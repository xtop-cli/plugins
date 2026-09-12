//! Example xtop WASM widget: top-process table.
//!
//! Demonstrates reading the capped process list from the snapshot and
//! rendering a multi-line text block (spans split on `\n` by the host).
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! Install: copy the `.wasm` into `~/.config/xtop/wasm/` and add `wasm-procs`
//! to a layout.

use xtop_wasm_guest::contract::{Border, DrawList, Manifest, Op, Rect, Span, State};
use xtop_wasm_guest::export_widget;

fn truncate(input: &str, max: usize) -> String {
    let mut out: String = input.chars().take(max).collect();
    if input.chars().count() > max && max > 1 {
        out.pop();
        out.push('…');
    }
    out
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1}G", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.0}M", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.0}K", bytes / KIB)
    } else {
        format!("{bytes:.0}B")
    }
}

export_widget! {
    manifest: || Manifest {
        name: "wasm-procs".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "top processes as a text table".to_string(),
        author: "xtop-cli".to_string(),
        max_processes: 40,
        ..Manifest::default()
    },
    render: |state: &State| {
        let mut list = DrawList::new();
        list.push(Op::Block {
            rect: Rect::full(state.width, state.height),
            border: Border::Rounded,
            title: Some("wasm procs".to_string()),
            fg: None,
            bg: None,
        });

        if state.width < 20 || state.height < 3 {
            return list;
        }

        let mut spans = vec![Span::new(format!(
            "{:>6} {:<18} {:>6} {:>7}",
            "PID", "NAME", "CPU%", "MEM"
        ))
        .bold()];

        let rows = state.height.saturating_sub(2) as usize;
        for process in state.snapshot.processes.iter().take(rows) {
            spans.push(Span::new("\n"));
            spans.push(Span::new(format!(
                "{:>6} {:<18} {:>6.1} {:>7}",
                process.pid,
                truncate(&process.name, 18),
                process.cpu_usage,
                format_bytes(process.memory)
            )));
        }

        list.push(Op::Text {
            rect: Rect::new(1, 1, state.width - 2, state.height - 2),
            spans,
            align: xtop_wasm_guest::contract::Align::Left,
            wrap: false,
        });
        list
    },
}
