//! `xtop-plugin-wasm` — runtime widget host for sandboxed WASM widgets.
//!
//! Widgets written in any language that compiles to `wasm32` (Rust via
//! [`xtop-wasm-guest`], or hand-written modules) are discovered in the user
//! config directory (`~/.config/xtop/wasm/` on Linux, overridable with
//! `XTOP_WASM_DIR`) and registered through the normal plugin widget path —
//! they keep precedence over compiled-in packs and can replace any widget
//! name. The compiled-in plugin/pack system is untouched: this host is an
//! optional kernel feature.
//!
//! # Lifecycle
//!
//! - **Load**: every `*.wasm` file is compiled and instantiated once at
//!   startup. A file that fails to load is logged and skipped.
//! - **Tick**: the host builds a [`contract::State`] from the plugin view,
//!   calls the guest `render`, and caches the resulting draw list. Guests run
//!   with a memory cap and a fuel budget.
//! - **Render**: the cached draw list is replayed onto the frame. Render
//!   never calls into the guest, so a slow guest can never stall a frame.
//! - **Hot reload**: a changed file mtime re-instantiates the guest on the
//!   next tick; the last good draw list stays on screen if reload fails.
//!
//! # Debugging
//!
//! `execute` actions exposed to agents/MCP: `status`, `render` (the cached
//! draw list as JSON) and `reload`. Guests may import `host.log(level, ptr,
//! len)` to write to xtop's stderr.

mod guest;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use guest::Guest;
use xtop_plugin_api::{
    Plugin, PluginCapability, PluginContext, PluginError, PluginManifest, PluginWidget,
};
use xtop_wasm_contract as contract;
use xtop_widget_replay::{replay, state_from_context};

/// Environment variable overriding the widget directory.
pub const DIR_ENV: &str = "XTOP_WASM_DIR";

/// The conventional directory name under the kernel config dir.
pub const DIR_NAME: &str = "wasm";

/// Discover `.wasm` widgets in `dir`, one plugin per module.
///
/// A missing directory yields an empty list (running without runtime widgets
/// is the normal state). Load errors are reported to stderr and skipped;
/// duplicate widget names keep the first file (sorted order) and warn.
pub fn discover(dir: &Path) -> Vec<Box<dyn Plugin>> {
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();

    let Ok(entries) = std::fs::read_dir(dir) else {
        return plugins;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("wasm"))
        .collect();
    paths.sort();

    for path in paths {
        match WasmWidgetPlugin::load(&path) {
            Ok(plugin) => {
                let name = plugin.manifest.id.clone();
                if !names.insert(name.clone()) {
                    eprintln!(
                        "[xtop] wasm widget '{}' in {} ignored: name already loaded",
                        name,
                        path.display()
                    );
                    continue;
                }
                plugins.push(Box::new(plugin));
            }
            Err(error) => {
                eprintln!(
                    "[xtop] wasm widget {} failed to load: {error}",
                    path.display()
                );
            }
        }
    }
    plugins
}

/// One loaded WASM widget.
struct WasmWidgetPlugin {
    path: PathBuf,
    manifest: PluginManifest,
    guest: Guest,
    max_processes: usize,
    tick: u64,
    /// Last widget-area size, updated by the render closure.
    size: Arc<Mutex<(u16, u16)>>,
    /// Last good draw list, replayed by the render closure.
    cache: Arc<Mutex<contract::DrawList>>,
    /// File mtime at load, for hot reload.
    mtime: Option<SystemTime>,
    /// Last tick error, to log once per distinct failure.
    last_error: Option<String>,
}

/// Debug report from [`inspect`].
#[derive(Debug, serde::Serialize)]
pub struct InspectReport {
    pub manifest: contract::Manifest,
    pub draw: contract::DrawList,
}

/// Load one module, ask for its manifest, and render a synthetic state.
///
/// No kernel, plugin context or terminal is involved: this is the
/// end-to-end check used by `examples/inspect.rs` and the local
/// `scripts/test-wasm-e2e.sh` (the same code path the kernel uses).
pub fn inspect(path: &Path) -> Result<InspectReport, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read error: {e}"))?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("wasm-widget")
        .to_string();
    let mut guest = Guest::load(&bytes, &name)?;
    let manifest = guest.call_manifest()?;
    let draw = guest.call_render(&synthetic_state())?;
    Ok(InspectReport { manifest, draw })
}

/// Minimal valid state for smoke-testing a guest.
fn synthetic_state() -> contract::State {
    use contract::*;
    State {
        tick: 1,
        unix_time: 1_700_000_000,
        width: 40,
        height: 12,
        config: RuntimeConfig {
            theme: "x".to_string(),
            layout: "inspect".to_string(),
            interval_ms: 1000,
            hostname: "inspect".to_string(),
        },
        alerts: Alerts {
            cpu_high: 90.0,
            mem_high: 90.0,
            disk_high: 90.0,
        },
        snapshot: Snapshot {
            cpus: vec![Cpu {
                name: "cpu0".to_string(),
                usage: 42.0,
                cpu_id: 0,
                frequency: 3600,
                governor: "schedutil".to_string(),
                temp_c: Some(55.0),
            }],
            memory: Memory {
                total: 16_000,
                used: 8_000,
                available: 8_000,
                free: 4_000,
                percent: 50.0,
            },
            swap: Swap {
                total: 0,
                used: 0,
                free: 0,
                percent: 0.0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![Process {
                pid: 1,
                name: "init".to_string(),
                cpu_usage: 1.0,
                memory: 1024,
                user_id: Some("root".to_string()),
                state: "S".to_string(),
                cmd: "init".to_string(),
                exe_path: None,
                parent_pid: None,
                cmd_full: vec![],
                start_time: 0,
                run_time: 1,
                effective_user_id: None,
                group_id: None,
                cwd: None,
                thread_count: 1,
                open_files: 0,
                open_files_limit: 0,
                disk_total_read_bytes: 0,
                disk_total_write_bytes: 0,
                environ: vec![],
                session_id: None,
            }],
            load: Load {
                one: 0.5,
                five: 0.4,
                fifteen: 0.3,
            },
            uptime: 1234,
            cpu_temp: 55.0,
            disk_io: vec![],
            batteries: vec![],
            gpus: vec![],
            sys: SysInfo::default(),
        },
    }
}

impl std::fmt::Debug for WasmWidgetPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmWidgetPlugin")
            .field("path", &self.path)
            .field("name", &self.manifest.id)
            .field("max_processes", &self.max_processes)
            .finish()
    }
}

impl WasmWidgetPlugin {
    fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read error: {e}"))?;
        let fallback_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("wasm-widget")
            .to_string();
        let mut guest = Guest::load(&bytes, &fallback_name)?;
        let manifest = guest.call_manifest()?;

        let name = if manifest.name.trim().is_empty() {
            fallback_name
        } else {
            manifest.name.trim().to_string()
        };
        if !manifest.api.is_empty() && manifest.api != contract::ABI_VERSION {
            eprintln!(
                "[xtop] wasm widget '{name}' targets contract {} (host speaks {}); loading anyway",
                manifest.api,
                contract::ABI_VERSION
            );
        }
        let max_processes = manifest.max_processes.clamp(1, 4096);

        let mtime = std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok());

        Ok(Self {
            path: path.to_path_buf(),
            manifest: PluginManifest {
                id: name.clone(),
                name: name.clone(),
                version: manifest.version,
                description: manifest.description,
                capabilities: vec![
                    PluginCapability::ReadSystemInfo,
                    PluginCapability::RenderWidgets,
                ],
            },
            guest,
            max_processes,
            tick: 0,
            size: Arc::new(Mutex::new((0, 0))),
            cache: Arc::new(Mutex::new(contract::DrawList::default())),
            mtime,
            last_error: None,
        })
    }

    /// Re-instantiate the guest when the file changed. Failures keep the
    /// current instance and are logged once.
    fn reload_if_changed(&mut self) {
        let Some(mtime) = std::fs::metadata(&self.path)
            .ok()
            .and_then(|meta| meta.modified().ok())
        else {
            return;
        };
        if Some(mtime) == self.mtime {
            return;
        }
        self.mtime = Some(mtime);

        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.report(format!("reload read error: {e}"));
                return;
            }
        };
        let mut guest = match Guest::load(&bytes, &self.manifest.id) {
            Ok(guest) => guest,
            Err(e) => {
                self.report(format!("reload failed, keeping previous module: {e}"));
                return;
            }
        };
        match guest.call_manifest() {
            Ok(manifest) => {
                if !manifest.name.trim().is_empty() && manifest.name.trim() != self.manifest.id {
                    eprintln!(
                        "[xtop] wasm widget '{}' reloaded as '{}' (layout references keep using the original name)",
                        self.manifest.id,
                        manifest.name.trim()
                    );
                }
                self.max_processes = manifest.max_processes.clamp(1, 4096);
                self.guest = guest;
                self.last_error = None;
                eprintln!("[xtop] wasm widget '{}' reloaded", self.manifest.id);
            }
            Err(e) => self.report(format!("reload manifest error: {e}")),
        }
    }

    /// Log a tick/reload failure once per distinct message.
    fn report(&mut self, error: String) {
        if self.last_error.as_deref() != Some(error.as_str()) {
            eprintln!("[wasm:{}] {error}", self.manifest.id);
            self.last_error = Some(error);
        }
    }

    fn tick_guest(&mut self, ctx: &PluginContext) {
        let (width, height) = self.size.lock().map(|size| *size).unwrap_or((0, 0));
        let state = match state_from_context(ctx, self.tick, width, height, self.max_processes) {
            Ok(state) => state,
            Err(e) => {
                self.report(format!("state build error: {e}"));
                return;
            }
        };
        match self.guest.call_render(&state) {
            Ok(list) => {
                if let Ok(mut cache) = self.cache.lock() {
                    *cache = list;
                }
                self.last_error = None;
            }
            Err(e) => self.report(e),
        }
        self.tick += 1;
    }
}

impl Plugin for WasmWidgetPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn on_tick(&mut self, ctx: &mut PluginContext) -> Result<(), PluginError> {
        self.reload_if_changed();
        self.tick_guest(ctx);
        Ok(())
    }

    fn widget(&self) -> Option<PluginWidget> {
        let cache = self.cache.clone();
        let size = self.size.clone();
        Some(PluginWidget {
            name: self.manifest.id.clone(),
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
                    "name": self.manifest.id,
                    "version": self.manifest.version,
                    "path": self.path.display().to_string(),
                    "ticks": self.tick,
                    "ops": ops,
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
            "reload" => {
                self.mtime = None;
                self.reload_if_changed();
                Ok(r#"{"reload":"requested"}"#.to_string())
            }
            other => Err(PluginError::UnknownAction(other.to_string())),
        }
    }
}
