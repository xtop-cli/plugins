//! Serializable contract between xtop's runtime widget hosts and external
//! guests (WASM modules or helper processes).
//!
//! The kernel renders widgets through [`xtop_widget_api`] renderers compiled
//! into the binary. This crate opens the same render surface to code that is
//! **not** compiled into the kernel: a runtime widget receives a [`State`]
//! snapshot (JSON on the wire) and answers with a [`DrawList`] — a small,
//! declarative list of drawing primitives. The Rust host replays that list
//! onto the ratatui frame, so guests never touch kernel types, ratatui, or
//! the terminal.
//!
//! Two hosts speak this contract today:
//!
//! - `xtop-plugin-wasm` — loads `*.wasm` modules in-process (wasmi sandbox).
//! - `xtop-plugin-external` — spawns a helper process and talks JSON lines.
//!
//! Both hosts are optional kernel features; the compiled-in widget packs are
//! untouched. The contract is deliberately dependency-light (serde only) so
//! Rust guests can compile it to `wasm32-unknown-unknown` without pulling
//! ratatui.
//!
//! # Wire shape
//!
//! `State` and `DrawList` are plain JSON. Coordinates in every op are
//! **relative to the widget's area** (the `Rect` the kernel hands the
//! widget), so a guest never needs to know where the widget lives on screen.
//!
//! ```json
//! {"ops":[
//!   {"op":"block","rect":{"x":0,"y":0,"width":30,"height":8},
//!    "border":"rounded","title":"CPU","fg":[200,200,200],"bg":null},
//!   {"op":"gauge","rect":{"x":1,"y":1,"width":28,"height":3},
//!    "ratio":0.42,"label":"42%","fg":[123,216,143],"bg":null,"border":null}
//! ]}
//! ```

use serde::{Deserialize, Serialize};

/// Version of the guest-facing ABI/contract. Guests declare the version they
/// were written against in their [`Manifest`]; hosts log a warning when it
/// does not match, but never refuse to load (forward-compatible by design).
pub const ABI_VERSION: &str = "1";

/// 24-bit RGB color, exactly as the widget contract uses it elsewhere.
pub type Color = [u8; 3];

/// Guest widget metadata, returned by `manifest()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Widget name as layouts reference it (e.g. `"wasm-clock"`). Must be
    /// unique across the running kernel; a later widget replaces an earlier
    /// one with the same name.
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Upper bound on the processes included in each [`State`] snapshot.
    /// Keeps the per-tick payload small for widgets that do not need a full
    /// process list.
    #[serde(default = "default_max_processes")]
    pub max_processes: usize,
    /// Contract version the guest was written against (see [`ABI_VERSION`]).
    #[serde(default)]
    pub api: String,
}

fn default_max_processes() -> usize {
    50
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            description: String::new(),
            author: String::new(),
            max_processes: default_max_processes(),
            api: ABI_VERSION.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Everything a guest may read on a tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// Monotonic tick counter since the widget was loaded.
    pub tick: u64,
    /// Unix time in seconds, host-provided (guests cannot read the clock in a
    /// portable sandbox).
    pub unix_time: u64,
    /// Width/height of the widget area as of the last render call. Zero
    /// before the first frame.
    pub width: u16,
    pub height: u16,
    /// Kernel runtime configuration (theme name, layout name, interval,
    /// hostname).
    pub config: RuntimeConfig,
    /// Current alert thresholds.
    pub alerts: Alerts,
    /// System sample for this tick.
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub theme: String,
    pub layout: String,
    pub interval_ms: u64,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alerts {
    pub cpu_high: f64,
    pub mem_high: f64,
    pub disk_high: f64,
}

/// Mirror of the kernel's `SystemSnapshot` (see `xtop-plugin-api`), made
/// serializable. Field names match the plugin data model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub cpus: Vec<Cpu>,
    pub memory: Memory,
    pub swap: Swap,
    pub disks: Vec<Disk>,
    pub networks: Vec<Network>,
    pub processes: Vec<Process>,
    pub load: Load,
    pub uptime: u64,
    pub cpu_temp: f64,
    pub disk_io: Vec<DiskIo>,
    pub batteries: Vec<Battery>,
    pub gpus: Vec<Gpu>,
    pub sys: SysInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cpu {
    pub name: String,
    pub usage: f64,
    pub cpu_id: usize,
    pub frequency: u64,
    pub governor: String,
    pub temp_c: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub free: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Swap {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disk {
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub used_space: u64,
    pub percent: f64,
    pub file_system: String,
    pub mount_options: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIo {
    pub name: String,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_speed: f64,
    pub write_speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
    pub received: u64,
    pub transmitted: u64,
    pub rx_speed: f64,
    pub tx_speed: f64,
    pub ip: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f64,
    pub memory: u64,
    pub user_id: Option<String>,
    pub state: String,
    pub cmd: String,
    pub exe_path: Option<String>,
    pub parent_pid: Option<u32>,
    pub cmd_full: Vec<String>,
    pub start_time: u64,
    pub run_time: u64,
    pub effective_user_id: Option<String>,
    pub group_id: Option<String>,
    pub cwd: Option<String>,
    pub thread_count: u64,
    pub open_files: u64,
    pub open_files_limit: u64,
    pub disk_total_read_bytes: u64,
    pub disk_total_write_bytes: u64,
    pub environ: Vec<String>,
    pub session_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Load {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Battery {
    pub name: String,
    pub percentage: f32,
    pub state: String,
    pub time_to_full: Option<u64>,
    pub time_to_empty: Option<u64>,
    pub health: f32,
    pub cycle_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub usage: f64,
    pub temperature: f32,
    pub memory_total: u64,
    pub memory_used: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SysInfo {
    pub hostname: String,
    pub os_version: String,
    pub kernel: String,
    pub desktop_env: String,
    pub shell: String,
    pub cpu_model: Option<String>,
    pub package_power_w: Option<f64>,
}

// ---------------------------------------------------------------------------
// Draw list
// ---------------------------------------------------------------------------

/// The guest's answer to a render request: an ordered list of primitives the
/// host replays onto the frame.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrawList {
    #[serde(default)]
    pub ops: Vec<Op>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, op: Op) -> &mut Self {
        self.ops.push(op);
        self
    }
}

/// A drawing primitive. Coordinates are relative to the widget area; the
/// host clips every rect to that area, so oversized rects are safe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// A bordered box, optionally titled.
    Block {
        rect: Rect,
        #[serde(default)]
        border: Border,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        fg: Option<Color>,
        #[serde(default)]
        bg: Option<Color>,
    },
    /// Styled text spans inside a rect.
    Text {
        rect: Rect,
        #[serde(default)]
        spans: Vec<Span>,
        #[serde(default)]
        align: Align,
        #[serde(default)]
        wrap: bool,
    },
    /// A horizontal gauge (ratatui `Gauge`).
    Gauge {
        rect: Rect,
        ratio: f64,
        #[serde(default)]
        label: Option<String>,
        /// Filled-bar color.
        #[serde(default)]
        fg: Option<Color>,
        /// Track/background color.
        #[serde(default)]
        bg: Option<Color>,
        #[serde(default)]
        border: Option<Border>,
    },
    /// A one-line bar (ratatui `LineGauge`).
    Bar {
        rect: Rect,
        ratio: f64,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        fg: Option<Color>,
        #[serde(default)]
        bg: Option<Color>,
        #[serde(default)]
        border: Option<Border>,
    },
    /// A sparkline over the rect.
    Sparkline {
        rect: Rect,
        #[serde(default)]
        data: Vec<f64>,
        #[serde(default)]
        fg: Option<Color>,
        #[serde(default)]
        bg: Option<Color>,
    },
    /// A line chart with one or more datasets.
    Chart {
        rect: Rect,
        #[serde(default)]
        datasets: Vec<Dataset>,
        #[serde(default)]
        x_bounds: Option<[f64; 2]>,
        #[serde(default)]
        y_bounds: Option<[f64; 2]>,
        #[serde(default)]
        border: Option<Border>,
        #[serde(default)]
        fg: Option<Color>,
        #[serde(default)]
        bg: Option<Color>,
        #[serde(default)]
        marker: Marker,
    },
}

/// Relative rect: `x`/`y` offset inside the widget area.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Rect {
    #[serde(default)]
    pub x: u16,
    #[serde(default)]
    pub y: u16,
    #[serde(default)]
    pub width: u16,
    #[serde(default)]
    pub height: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The whole widget area.
    pub fn full(width: u16, height: u16) -> Self {
        Self::new(0, 0, width, height)
    }
}

/// Border look; mirrors `xtop-widget-api`'s `WidgetBorders` serde names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Border {
    #[default]
    Native,
    Rounded,
    Double,
    Plain,
    Ascii,
}

/// Text alignment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Chart marker; mirrors `xtop-widget-api`'s `ChartCharset`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Marker {
    #[default]
    Braille,
    Dot,
    Block,
    HalfBlock,
    Bar,
}

/// A styled text run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default)]
    pub fg: Option<Color>,
    #[serde(default)]
    pub bg: Option<Color>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underlined: bool,
    #[serde(default)]
    pub dim: bool,
}

impl Span {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underlined: false,
            dim: false,
        }
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn underlined(mut self) -> Self {
        self.underlined = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
}

/// One chart series. Points are `[x, y]` pairs (Y grows upward).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub color: Option<Color>,
}

// ---------------------------------------------------------------------------
// External-process wire protocol
// ---------------------------------------------------------------------------

/// Host → guest request (one JSON object per line).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Ask for the guest [`Manifest`]. Sent once at load.
    Manifest,
    /// Ask for a [`DrawList`] for the given state. Sent once per tick.
    Render { state: Box<State> },
    /// Ask the guest to exit cleanly. Sent on shutdown.
    Shutdown,
}

/// Guest → host response (one JSON object per line).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Manifest {
        manifest: Manifest,
    },
    Draw {
        ops: Vec<Op>,
    },
    /// Free-form diagnostic line; the host prints it to stderr.
    Log {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> State {
        State {
            tick: 7,
            unix_time: 1_700_000_000,
            width: 40,
            height: 12,
            config: RuntimeConfig {
                theme: "tokio".into(),
                layout: "WASM Demo".into(),
                interval_ms: 1000,
                hostname: "box".into(),
            },
            alerts: Alerts {
                cpu_high: 90.0,
                mem_high: 90.0,
                disk_high: 90.0,
            },
            snapshot: Snapshot {
                cpus: vec![Cpu {
                    name: "cpu0".into(),
                    usage: 42.0,
                    cpu_id: 0,
                    frequency: 3600,
                    governor: "schedutil".into(),
                    temp_c: Some(55.0),
                }],
                memory: Memory {
                    total: 100,
                    used: 50,
                    available: 50,
                    free: 50,
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
                processes: vec![],
                load: Load {
                    one: 0.5,
                    five: 0.4,
                    fifteen: 0.3,
                },
                uptime: 123,
                cpu_temp: 55.0,
                disk_io: vec![],
                batteries: vec![],
                gpus: vec![],
                sys: SysInfo::default(),
            },
        }
    }

    #[test]
    fn state_round_trips_through_json() {
        let state = sample_state();
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tick, 7);
        assert_eq!(back.snapshot.cpus[0].usage, 42.0);
        assert_eq!(back.config.theme, "tokio");
    }

    #[test]
    fn draw_list_round_trips_with_tagged_ops() {
        let list = DrawList {
            ops: vec![
                Op::Block {
                    rect: Rect::new(0, 0, 30, 8),
                    border: Border::Rounded,
                    title: Some("CPU".into()),
                    fg: Some([200, 200, 200]),
                    bg: None,
                },
                Op::Gauge {
                    rect: Rect::new(1, 1, 28, 3),
                    ratio: 0.42,
                    label: Some("42%".into()),
                    fg: Some([123, 216, 143]),
                    bg: None,
                    border: None,
                },
            ],
        };
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.contains(r#""op":"block""#));
        assert!(json.contains(r#""op":"gauge""#));
        let back: DrawList = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ops.len(), 2);
        match &back.ops[0] {
            Op::Block { border, .. } => assert_eq!(*border, Border::Rounded),
            _ => panic!("wrong op"),
        }
    }

    #[test]
    fn manifest_defaults_are_guest_friendly() {
        let manifest: Manifest = serde_json::from_str(r#"{"name":"tiny"}"#).unwrap();
        assert_eq!(manifest.max_processes, 50);
        assert!(manifest.api.is_empty());
    }

    #[test]
    fn request_and_response_are_tagged() {
        let req = serde_json::to_string(&Request::Manifest).unwrap();
        assert_eq!(req, r#"{"type":"manifest"}"#);
        let req = serde_json::to_string(&Request::Render {
            state: Box::new(sample_state()),
        })
        .unwrap();
        assert!(req.starts_with(r#"{"type":"render","state":"#));

        let res: Response =
            serde_json::from_str(r#"{"type":"draw","ops":[{"op":"sparkline","rect":{"width":4,"height":1},"data":[1.0,2.0]}]}"#)
                .unwrap();
        match res {
            Response::Draw { ops } => assert_eq!(ops.len(), 1),
            _ => panic!("wrong response"),
        }
    }
}
