//! Example xtop WASM widget: CPU gauge + sparkline.
//!
//! Demonstrates a guest that keeps scratch state across ticks: the average
//! CPU usage is appended to a bounded history held in the guest, then drawn
//! as a sparkline. Colors follow the kernel's alert thresholds, which arrive
//! in every state snapshot.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! Install: copy the `.wasm` into `~/.config/xtop/wasm/` and add `wasm-cpu`
//! to a layout.

#![allow(static_mut_refs)]

use xtop_wasm_guest::contract::{Border, Color, DrawList, Manifest, Op, Rect, Span, State};
use xtop_wasm_guest::export_widget;

/// Bounded guest-side history (the host does not send histories to guests).
static mut HISTORY: Vec<f64> = Vec::new();
const MAX_POINTS: usize = 120;

const GREEN: Color = [123, 216, 143];
const YELLOW: Color = [252, 229, 102];
const RED: Color = [252, 97, 141];
const VIOLET: Color = [148, 138, 227];

fn usage_color(usage: f64, threshold: f64) -> Color {
    if usage >= threshold {
        RED
    } else if usage >= threshold * 0.75 {
        YELLOW
    } else {
        GREEN
    }
}

export_widget! {
    manifest: || Manifest {
        name: "wasm-cpu".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "average CPU gauge and history sparkline".to_string(),
        author: "xtop-cli".to_string(),
        max_processes: 1,
        ..Manifest::default()
    },
    render: |state: &State| {
        let mut list = DrawList::new();
        list.push(Op::Block {
            rect: Rect::full(state.width, state.height),
            border: Border::Rounded,
            title: Some("wasm cpu".to_string()),
            fg: None,
            bg: None,
        });

        if state.width < 10 || state.height < 4 {
            return list;
        }

        let cpus = &state.snapshot.cpus;
        let usage = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|cpu| cpu.usage).sum::<f64>() / cpus.len() as f64
        };

        unsafe {
            HISTORY.push(usage);
            while HISTORY.len() > MAX_POINTS {
                HISTORY.remove(0);
            }
        }

        let color = usage_color(usage, state.alerts.cpu_high);
        list.push(Op::Gauge {
            rect: Rect::new(1, 1, state.width - 2, 3),
            ratio: (usage / 100.0).clamp(0.0, 1.0),
            label: Some(format!("{usage:.1}%")),
            fg: Some(color),
            bg: None,
            border: None,
        });

        if state.height > 6 {
            let data = unsafe { HISTORY.clone() };
            list.push(Op::Sparkline {
                rect: Rect::new(1, 4, state.width - 2, state.height - 5),
                data,
                fg: Some(VIOLET),
                bg: None,
            });
        } else if state.height > 4 {
            list.push(Op::Text {
                rect: Rect::new(1, 4, state.width - 2, 1),
                spans: vec![Span::new(format!("load {:.2}", state.snapshot.load.one)).dim()],
                align: xtop_wasm_guest::contract::Align::Left,
                wrap: false,
            });
        }
        list
    },
}
