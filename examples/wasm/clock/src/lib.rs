//! Example xtop WASM widget: a UTC clock.
//!
//! Demonstrates the minimal guest: one manifest, one render function, no
//! state kept across ticks. The clock is derived from `state.unix_time`
//! (guests cannot read the host clock portably) and the uptime line comes
//! from the system snapshot.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! Install: copy `target/wasm32-unknown-unknown/release/*.wasm` into
//! `~/.config/xtop/wasm/` and add `wasm-clock` to a layout.

use xtop_wasm_guest::contract::{Align, Border, DrawList, Manifest, Op, Rect, Span, State};
use xtop_wasm_guest::export_widget;

/// Compact "1d 02h 03m" style duration.
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h {minutes:02}m")
    } else {
        format!("{hours:02}h {minutes:02}m")
    }
}

export_widget! {
    manifest: || Manifest {
        name: "wasm-clock".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "UTC clock from the host-provided unix time".to_string(),
        author: "xtop-cli".to_string(),
        max_processes: 1,
        ..Manifest::default()
    },
    render: |state: &State| {
        let mut list = DrawList::new();
        list.push(Op::Block {
            rect: Rect::full(state.width, state.height),
            border: Border::Rounded,
            title: Some("wasm clock".to_string()),
            fg: None,
            bg: None,
        });

        if state.width < 8 || state.height < 3 {
            return list;
        }

        let seconds = state.unix_time;
        let clock = format!(
            "{:02}:{:02}:{:02}",
            (seconds / 3_600) % 24,
            (seconds / 60) % 60,
            seconds % 60
        );
        list.push(Op::Text {
            rect: Rect::new(1, 1, state.width - 2, 1),
            spans: vec![Span::new(clock).fg([90, 212, 230]).bold()],
            align: Align::Center,
            wrap: false,
        });

        if state.height > 3 {
            list.push(Op::Text {
                rect: Rect::new(1, 2, state.width - 2, 1),
                spans: vec![Span::new(format!(
                    "up {}",
                    format_uptime(state.snapshot.uptime)
                ))
                .dim()],
                align: Align::Center,
                wrap: false,
            });
        }
        list
    },
}
