//! Host tests: the ABI is exercised with hand-written WAT modules (no wasm32
//! target required) plus a real plugin-context tick.

use std::path::PathBuf;

use super::*;
use xtop_plugin_api::{
    AlertThresholds, CpuInfo, HostState, LoadAvg, MemoryInfo, PluginCapability, RuntimeConfig,
    SwapInfo, SystemInfo, SystemSnapshot,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct FakeHost {
    snapshot: SystemSnapshot,
}

impl FakeHost {
    fn new() -> Self {
        Self {
            snapshot: sample_snapshot(),
        }
    }
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
            usage: 12.5,
            cpu_id: 0,
            frequency: 3600,
            governor: "schedutil".into(),
            temp_c: Some(48.0),
        }],
        memory: MemoryInfo {
            total: 1000,
            used: 400,
            available: 600,
            free: 500,
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
            one: 0.5,
            five: 0.4,
            fifteen: 0.3,
        },
        uptime: 100,
        cpu_temp: 48.0,
        disk_io: vec![],
        batteries: vec![],
        gpus: vec![],
        sys_info: SystemInfo::default(),
    }
}

fn escape_wat(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A WAT module that answers `manifest`/`render` with static JSON blobs.
fn wat_widget(manifest: &str, draw: &str) -> Vec<u8> {
    // Lengths must be the unescaped byte counts: WAT escapes shrink back to
    // one byte in the data segment.
    let mlen = manifest.len();
    let dlen = draw.len();
    let manifest = escape_wat(manifest);
    let draw = escape_wat(draw);
    let wat = format!(
        r#"
        (module
            (memory (export "memory") 4)
            (data (i32.const 1024) "{manifest}")
            (data (i32.const 4096) "{draw}")
            (global $len (mut i32) (i32.const 0))
            (func (export "result_len") (result i32) (global.get $len))
            (func (export "alloc") (param i32) (result i32) (i32.const 8192))
            (func (export "dealloc") (param i32 i32))
            (func (export "manifest") (result i32)
                (global.set $len (i32.const {mlen}))
                (i32.const 1024))
            (func (export "render") (param i32 i32) (result i32)
                (global.set $len (i32.const {dlen}))
                (i32.const 4096))
        )"#,
        mlen = mlen,
        dlen = dlen,
    );
    wat::parse_str(&wat).expect("test WAT must compile")
}

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xtop-wasm-test-{}-{name}.wasm", std::process::id()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn wat_guest_round_trips_manifest_and_draw_list() {
    let manifest = r#"{"name":"wat-widget","version":"9.9.9","description":"wat","max_processes":10,"api":"1"}"#;
    let draw = r#"{"ops":[{"op":"text","rect":{"x":0,"y":0,"width":10,"height":1},"spans":[{"text":"hi from wat"}]}]}"#;
    let path = temp_file("roundtrip");
    std::fs::write(&path, wat_widget(manifest, draw)).unwrap();

    let mut plugin = WasmWidgetPlugin::load(&path).unwrap();
    assert_eq!(plugin.manifest.id, "wat-widget");
    assert_eq!(plugin.max_processes, 10);
    assert!(plugin
        .manifest
        .capabilities
        .contains(&PluginCapability::RenderWidgets));
    assert!(plugin
        .manifest
        .capabilities
        .contains(&PluginCapability::ReadSystemInfo));

    let mut host = FakeHost::new();
    let caps = plugin.manifest.capabilities.clone();
    let mut ctx = PluginContext::new(
        &mut host,
        std::env::temp_dir().join("xtop-wasm-test-data"),
        caps,
    );
    plugin.tick_guest(&ctx);

    let rendered = plugin.execute(&mut ctx, "render", "").unwrap();
    let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(value["ops"].as_array().unwrap().len(), 1);
    assert_eq!(value["ops"][0]["op"], "text");
    assert_eq!(value["ops"][0]["spans"][0]["text"], "hi from wat");

    let status: serde_json::Value =
        serde_json::from_str(&plugin.execute(&mut ctx, "status", "").unwrap()).unwrap();
    assert_eq!(status["name"], "wat-widget");
    assert_eq!(status["version"], "9.9.9");
    assert_eq!(status["ops"], 1);
    assert_eq!(status["ticks"], 1);
    assert!(status["last_error"].is_null());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn widget_plugin_exposes_a_renderer_with_the_manifest_name() {
    let path = temp_file("widget");
    std::fs::write(
        &path,
        wat_widget(r#"{"name":"render-me"}"#, r#"{"ops":[]}"#),
    )
    .unwrap();
    let plugin = WasmWidgetPlugin::load(&path).unwrap();
    let widget = plugin.widget().expect("plugin must expose a widget");
    assert_eq!(widget.name, "render-me");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_memory_export_is_rejected() {
    let bytes = wat::parse_str("(module)").unwrap();
    let error = Guest::load(&bytes, "empty").unwrap_err();
    assert!(error.contains("memory"), "unexpected error: {error}");
}

#[test]
fn runaway_guest_is_stopped_by_the_fuel_budget() {
    let manifest = r#"{"name":"looper"}"#;
    let manifest = escape_wat(manifest);
    let wat = format!(
        r#"
        (module
            (memory (export "memory") 1)
            (data (i32.const 1024) "{manifest}")
            (global $len (mut i32) (i32.const 0))
            (func (export "result_len") (result i32) (global.get $len))
            (func (export "alloc") (param i32) (result i32) (i32.const 2048))
            (func (export "dealloc") (param i32 i32))
            (func (export "manifest") (result i32)
                (global.set $len (i32.const {mlen}))
                (i32.const 1024))
            (func (export "render") (param i32 i32) (result i32)
                (loop $spin (br $spin))
                (i32.const 0))
        )"#,
        mlen = manifest.len(),
    );
    let bytes = wat::parse_str(&wat).unwrap();
    let mut guest = Guest::load_with_fuel(&bytes, "looper", 500_000).unwrap();
    let state = xtop_wasm_contract::State {
        tick: 1,
        unix_time: 0,
        width: 10,
        height: 5,
        config: xtop_wasm_contract::RuntimeConfig {
            theme: "x".into(),
            layout: "l".into(),
            interval_ms: 1000,
            hostname: "h".into(),
        },
        alerts: xtop_wasm_contract::Alerts {
            cpu_high: 90.0,
            mem_high: 90.0,
            disk_high: 90.0,
        },
        snapshot: xtop_widget_replay::snapshot_to_contract(&sample_snapshot(), 10),
    };
    let error = guest.call_render(&state).unwrap_err();
    assert!(
        error.contains("trap") || error.contains("fuel"),
        "expected a fuel trap, got: {error}"
    );
}

#[test]
fn discover_skips_missing_dirs_and_broken_files() {
    let dir = std::env::temp_dir().join(format!("xtop-wasm-discover-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("note.txt"), b"not wasm").unwrap();
    std::fs::write(dir.join("broken.wasm"), b"\0asm not really").unwrap();
    assert!(discover(&dir).is_empty());
    assert!(discover(&dir.join("does-not-exist")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discover_loads_every_valid_module() {
    let dir = std::env::temp_dir().join(format!("xtop-wasm-discover-ok-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a.wasm"),
        wat_widget(r#"{"name":"alpha"}"#, r#"{"ops":[]}"#),
    )
    .unwrap();
    std::fs::write(
        dir.join("b.wasm"),
        wat_widget(r#"{"name":"beta"}"#, r#"{"ops":[]}"#),
    )
    .unwrap();
    let plugins = discover(&dir);
    let mut names: Vec<String> = plugins.iter().map(|p| p.manifest().id).collect();
    names.sort();
    assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}
