//! `xtop-plugin-external` — runtime widget host for helper processes.
//!
//! Any language with a runtime can provide a widget: write a small program
//! that reads line-delimited JSON [`Request`](xtop_wasm_contract::Request)
//! objects on stdin and writes one [`Response`](xtop_wasm_contract::Response)
//! per line on stdout. The host discovers `<name>.json` descriptor files in
//! the user config directory (`~/.config/xtop/external/` on Linux, overridable
//! with `XTOP_EXTERNAL_DIR`) and registers one plugin per descriptor.
//!
//! This is the same process-boundary model as the MCP extension: the guest
//! runs with the user's permissions and is the sandbox. WASM widgets are the
//! in-process, capability-limited alternative.
//!
//! # Descriptor
//!
//! ```json
//! {
//!   "name": "lua-clock",
//!   "description": "clock in Lua",
//!   "command": ["lua", "/home/me/widgets/clock.lua"],
//!   "timeout_ms": 2000,
//!   "max_processes": 20
//! }
//! ```
//!
//! # Lifecycle
//!
//! - **Load**: the process is spawned and asked for its manifest once.
//! - **Tick**: the state JSON goes in, the draw list comes back and is
//!   cached. Reads are bounded by `timeout_ms`; on timeout the last good draw
//!   list stays on screen and the failure is logged once.
//! - **Render**: replay only, never touches the process.
//! - **Shutdown**: the host sends `{"type":"shutdown"}` and kills the child.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use xtop_plugin_api::{
    Plugin, PluginCapability, PluginContext, PluginError, PluginManifest, PluginWidget,
};
use xtop_wasm_contract as contract;
use xtop_widget_replay::{replay, state_from_context};

/// Environment variable overriding the descriptor directory.
pub const DIR_ENV: &str = "XTOP_EXTERNAL_DIR";

/// The conventional directory name under the kernel config dir.
pub const DIR_NAME: &str = "external";

const DEFAULT_TIMEOUT_MS: u64 = 2000;

/// One `<name>.json` descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalConfig {
    /// Widget name as layouts reference it. Defaults to the file stem.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Command and arguments, e.g. `["lua", "widget.lua"]`.
    pub command: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_processes")]
    pub max_processes: usize,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_max_processes() -> usize {
    50
}

/// Discover descriptor files in `dir`, one plugin per widget.
pub fn discover(dir: &Path) -> Vec<Box<dyn Plugin>> {
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return plugins;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    for path in paths {
        match ExternalWidgetPlugin::load(&path) {
            Ok(plugin) => plugins.push(Box::new(plugin)),
            Err(error) => eprintln!(
                "[xtop] external widget {} failed to load: {error}",
                path.display()
            ),
        }
    }
    plugins
}

/// One spawned helper process.
struct ExternalWidgetPlugin {
    path: PathBuf,
    config: ExternalConfig,
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    cache: Arc<Mutex<contract::DrawList>>,
    size: Arc<Mutex<(u16, u16)>>,
    tick: u64,
    last_error: Option<String>,
}

impl std::fmt::Debug for ExternalWidgetPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalWidgetPlugin")
            .field("path", &self.path)
            .field("name", &self.config.name)
            .field("command", &self.config.command)
            .finish()
    }
}

impl ExternalWidgetPlugin {
    fn load(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
        let mut config: ExternalConfig =
            serde_json::from_str(&data).map_err(|e| format!("invalid descriptor: {e}"))?;
        if config.name.trim().is_empty() {
            config.name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("external-widget")
                .to_string();
        }
        if config.command.is_empty() {
            return Err("descriptor has an empty `command`".to_string());
        }
        config.timeout_ms = config.timeout_ms.clamp(100, 60_000);
        config.max_processes = config.max_processes.clamp(1, 4096);

        let mut child = Command::new(&config.command[0])
            .args(&config.command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn `{}` failed: {e}", config.command[0]))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "child stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "child stdout unavailable".to_string())?;

        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut plugin = Self {
            path: path.to_path_buf(),
            config,
            child,
            stdin,
            lines,
            cache: Arc::new(Mutex::new(contract::DrawList::default())),
            size: Arc::new(Mutex::new((0, 0))),
            tick: 0,
            last_error: None,
        };

        // Ask for the manifest once; a process that cannot answer is broken.
        match plugin.request(&contract::Request::Manifest) {
            Ok(contract::Response::Manifest { manifest }) => {
                if !manifest.description.is_empty() {
                    plugin.config.description = manifest.description;
                }
                if plugin.config.max_processes == default_max_processes()
                    && manifest.max_processes != default_max_processes()
                {
                    plugin.config.max_processes = manifest.max_processes.clamp(1, 4096);
                }
            }
            Ok(_) => return Err("process answered the manifest request with a draw list".into()),
            Err(error) => return Err(format!("manifest request failed: {error}")),
        }
        Ok(plugin)
    }

    /// Send one request and wait for a non-log response.
    fn request(&mut self, request: &contract::Request) -> Result<contract::Response, String> {
        let line = serde_json::to_string(request).map_err(|e| format!("encode error: {e}"))?;
        writeln!(self.stdin, "{line}").map_err(|e| format!("write to process failed: {e}"))?;
        self.stdin
            .flush()
            .map_err(|e| format!("flush to process failed: {e}"))?;

        let timeout = Duration::from_millis(self.config.timeout_ms);
        loop {
            match self.lines.recv_timeout(timeout) {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<contract::Response>(&line) {
                        Ok(contract::Response::Log { message }) => {
                            eprintln!("[external:{}] {message}", self.config.name);
                        }
                        Ok(response) => return Ok(response),
                        Err(e) => {
                            return Err(format!("invalid response line: {e}"));
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "timed out after {} ms waiting for the process",
                        self.config.timeout_ms
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("process exited unexpectedly".to_string());
                }
            }
        }
    }

    fn report(&mut self, error: String) {
        if self.last_error.as_deref() != Some(error.as_str()) {
            eprintln!("[external:{}] {error}", self.config.name);
            self.last_error = Some(error);
        }
    }

    fn tick_guest(&mut self, ctx: &PluginContext) {
        let (width, height) = self.size.lock().map(|size| *size).unwrap_or((0, 0));
        let state =
            match state_from_context(ctx, self.tick, width, height, self.config.max_processes) {
                Ok(state) => state,
                Err(e) => {
                    self.report(format!("state build error: {e}"));
                    return;
                }
            };
        match self.request(&contract::Request::Render {
            state: Box::new(state),
        }) {
            Ok(contract::Response::Draw { ops }) => {
                if let Ok(mut cache) = self.cache.lock() {
                    *cache = contract::DrawList { ops };
                }
                self.last_error = None;
            }
            Ok(other) => self.report(format!("unexpected response: {other:?}")),
            Err(e) => self.report(e),
        }
        self.tick += 1;
    }

    fn shutdown(&mut self) {
        if let Ok(line) = serde_json::to_string(&contract::Request::Shutdown) {
            let _ = writeln!(self.stdin, "{line}");
            let _ = self.stdin.flush();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ExternalWidgetPlugin {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Plugin for ExternalWidgetPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: self.config.name.clone(),
            name: self.config.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: self.config.description.clone(),
            capabilities: vec![
                PluginCapability::ReadSystemInfo,
                PluginCapability::RenderWidgets,
            ],
        }
    }

    fn on_tick(&mut self, ctx: &mut PluginContext) -> Result<(), PluginError> {
        self.tick_guest(ctx);
        Ok(())
    }

    fn widget(&self) -> Option<PluginWidget> {
        let cache = self.cache.clone();
        let size = self.size.clone();
        Some(PluginWidget {
            name: self.config.name.clone(),
            render: Arc::new(move |frame, _state, area| {
                if let Ok(mut current) = size.lock() {
                    *current = (area.width, area.height);
                }
                if let Ok(list) = cache.lock() {
                    replay(frame, area, &list);
                }
            }),
        })
    }

    fn execute(
        &mut self,
        _ctx: &mut PluginContext,
        action: &str,
        _params: &str,
    ) -> Result<String, PluginError> {
        match action {
            "status" => {
                let ops = self.cache.lock().map(|list| list.ops.len()).unwrap_or(0);
                Ok(serde_json::json!({
                    "name": self.config.name,
                    "path": self.path.display().to_string(),
                    "command": self.config.command,
                    "ticks": self.tick,
                    "ops": ops,
                    "timeout_ms": self.config.timeout_ms,
                    "last_error": self.last_error,
                })
                .to_string())
            }
            "render" => {
                let list = self
                    .cache
                    .lock()
                    .map_err(|_| PluginError::Recoverable("draw cache poisoned".to_string()))?;
                serde_json::to_string(&*list)
                    .map_err(|e| PluginError::Recoverable(format!("encode error: {e}")))
            }
            "reload" => Err(PluginError::Recoverable(
                "external widgets reload by restarting xtop".to_string(),
            )),
            other => Err(PluginError::UnknownAction(other.to_string())),
        }
    }

    fn on_disable(&mut self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        self.shutdown();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xtop_plugin_api::{
        AlertThresholds, CpuInfo, HostState, LoadAvg, MemoryInfo, RuntimeConfig, SwapInfo,
        SystemInfo, SystemSnapshot,
    };

    struct FakeHost {
        snapshot: SystemSnapshot,
    }

    impl HostState for FakeHost {
        fn snapshot(&self) -> SystemSnapshot {
            self.snapshot.clone()
        }
        fn system_info(&self) -> SystemInfo {
            self.snapshot.sys_info.clone()
        }
        fn kill_process(&mut self, _pid: u32) -> bool {
            false
        }
        fn set_alert_thresholds(&mut self, _cpu: f64, _mem: f64, _disk: f64) {}
        fn alerts(&self) -> AlertThresholds {
            AlertThresholds {
                cpu_high: 90.0,
                mem_high: 90.0,
                disk_high: 90.0,
            }
        }
        fn config(&self) -> RuntimeConfig {
            RuntimeConfig {
                theme: "tokio".into(),
                layout: "WASM Demo".into(),
                interval_ms: 1000,
                hostname: "test-box".into(),
            }
        }
        fn set_theme_by_name(&mut self, _name: &str) -> bool {
            false
        }
        fn set_layout_by_name(&mut self, _name: &str) -> bool {
            false
        }
        fn set_update_interval_ms(&mut self, _ms: u64) {}
    }

    fn sample_snapshot() -> SystemSnapshot {
        SystemSnapshot {
            cpus: vec![CpuInfo {
                name: "cpu0".into(),
                usage: 10.0,
                cpu_id: 0,
                frequency: 3200,
                governor: "performance".into(),
                temp_c: None,
            }],
            memory: MemoryInfo {
                total: 100,
                used: 40,
                available: 60,
                free: 50,
                percent: 40.0,
            },
            swap: SwapInfo {
                total: 0,
                used: 0,
                free: 0,
                percent: 0.0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
            load_avg: LoadAvg {
                one: 0.1,
                five: 0.2,
                fifteen: 0.3,
            },
            uptime: 10,
            cpu_temp: 0.0,
            disk_io: vec![],
            batteries: vec![],
            gpus: vec![],
            sys_info: SystemInfo::default(),
        }
    }

    const PY_WIDGET: &str = r#"
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    kind = req.get("type")
    if kind == "manifest":
        send({"type": "manifest", "manifest": {"name": "py-test", "description": "python test widget"}})
    elif kind == "render":
        state = req["state"]
        send({"type": "draw", "ops": [
            {"op": "text", "rect": {"x": 0, "y": 0, "width": 20, "height": 1},
             "spans": [{"text": "tick %d" % state["tick"]}]}
        ]})
    elif kind == "shutdown":
        break
"#;

    fn python_available() -> bool {
        Command::new("python3").arg("--version").output().is_ok()
    }

    #[test]
    fn python_widget_round_trips_over_stdio() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("xtop-external-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("widget.py");
        std::fs::write(&script, PY_WIDGET).unwrap();
        let descriptor = dir.join("py-test.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "py-test",
                "description": "python test widget",
                "command": ["python3", script.display().to_string()],
                "timeout_ms": 5000,
            })
            .to_string(),
        )
        .unwrap();

        let plugins = discover(&dir);
        assert_eq!(plugins.len(), 1, "descriptor must load one plugin");

        let mut host = FakeHost {
            snapshot: sample_snapshot(),
        };
        let caps = vec![
            PluginCapability::ReadSystemInfo,
            PluginCapability::RenderWidgets,
        ];
        let mut ctx = PluginContext::new(&mut host, dir.clone(), caps);

        // `discover` returns trait objects; drive it through the trait.
        let mut plugin = plugins.into_iter().next().unwrap();
        plugin.on_tick(&mut ctx).unwrap();

        let rendered = plugin.execute(&mut ctx, "render", "").unwrap();
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["ops"][0]["op"], "text");
        assert_eq!(value["ops"][0]["spans"][0]["text"], "tick 0");

        let status: serde_json::Value =
            serde_json::from_str(&plugin.execute(&mut ctx, "status", "").unwrap()).unwrap();
        assert_eq!(status["name"], "py-test");
        assert_eq!(status["ops"], 1);

        drop(plugin);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_yields_no_plugins() {
        let dir = std::env::temp_dir().join("xtop-external-does-not-exist");
        assert!(discover(&dir).is_empty());
    }

    #[test]
    fn broken_descriptor_is_skipped() {
        let dir = std::env::temp_dir().join(format!("xtop-external-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.json"), "{ not json").unwrap();
        std::fs::write(dir.join("empty.json"), r#"{"command":[]}"#).unwrap();
        assert!(discover(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
