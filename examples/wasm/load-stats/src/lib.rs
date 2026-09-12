//! Example xtop WASM widget: load average with streaming statistics.
//!
//! Demonstrates math done inside a sandboxed WASM guest: it keeps its own
//! history of the 1-minute load average and renders
//!
//! - a gauge of the current load,
//! - a chart with the raw history and its exponential moving average (EMA),
//! - a footer with mean, standard deviation and the z-score of the current
//!   sample (how many sigmas away from the mean it is).
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! Install: copy the `.wasm` into `~/.config/xtop/wasm/` and add
//! `wasm-load-stats` to a layout.

#![allow(static_mut_refs)]

use xtop_wasm_guest::contract::{Align, Border, Color, Dataset, DrawList, Manifest, Marker, Op, Rect, Span, State};
use xtop_wasm_guest::export_widget;

static mut HISTORY: Vec<f64> = Vec::new();
const MAX_POINTS: usize = 180;
const EMA_ALPHA: f64 = 0.2;

const CYAN: Color = [90, 212, 230];
const VIOLET: Color = [148, 138, 227];
const GREEN: Color = [123, 216, 143];
const YELLOW: Color = [252, 229, 102];
const RED: Color = [252, 97, 141];
const DIM: Color = [140, 140, 140];

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn stdev(values: &[f64], avg: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let variance = values
        .iter()
        .map(|value| (value - avg) * (value - avg))
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt()
}

fn ema_series(values: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(values.len());
    let mut current = 0.0;
    for (index, value) in values.iter().enumerate() {
        current = if index == 0 {
            *value
        } else {
            EMA_ALPHA * value + (1.0 - EMA_ALPHA) * current
        };
        out.push(current);
    }
    out
}

fn load_color(load: f64, cores: f64) -> Color {
    if cores <= 0.0 {
        return GREEN;
    }
    let ratio = load / cores;
    if ratio >= 1.0 {
        RED
    } else if ratio >= 0.7 {
        YELLOW
    } else {
        GREEN
    }
}

export_widget! {
    manifest: || Manifest {
        name: "wasm-load-stats".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "load average with EMA and z-score statistics".to_string(),
        author: "xtop-cli".to_string(),
        max_processes: 1,
        ..Manifest::default()
    },
    render: |state: &State| {
        let mut list = DrawList::new();
        list.push(Op::Block {
            rect: Rect::full(state.width, state.height),
            border: Border::Rounded,
            title: Some("wasm load stats".to_string()),
            fg: None,
            bg: None,
        });

        if state.width < 20 || state.height < 7 {
            return list;
        }

        let load = state.snapshot.load.one;
        let cores = state.snapshot.cpus.len().max(1) as f64;
        let color = load_color(load, cores);

        unsafe {
            HISTORY.push(load);
            while HISTORY.len() > MAX_POINTS {
                HISTORY.remove(0);
            }
        }

        let samples = unsafe { HISTORY.clone() };
        if samples.len() < 2 {
            list.push(Op::Text {
                rect: Rect::new(1, 2, state.width - 2, 1),
                spans: vec![Span::new("collecting samples...").dim()],
                align: Align::Center,
                wrap: false,
            });
            return list;
        }

        let avg = mean(&samples);
        let sigma = stdev(&samples, avg);
        let z_score = if sigma > 0.0001 { (load - avg) / sigma } else { 0.0 };
        let ema = ema_series(&samples);
        let y_max = samples.iter().cloned().fold(1.0_f64, f64::max) * 1.2;
        let x_end = (samples.len() - 1) as f64;

        list.push(Op::Gauge {
            rect: Rect::new(1, 1, state.width - 2, 3),
            ratio: (load / y_max).clamp(0.0, 1.0),
            label: Some(format!("load {load:.2} / {cores:.0} cores")),
            fg: Some(color),
            bg: None,
            border: None,
        });

        let raw: Vec<[f64; 2]> = samples
            .iter()
            .enumerate()
            .map(|(i, value)| [i as f64, *value])
            .collect();
        let smooth: Vec<[f64; 2]> = ema
            .iter()
            .enumerate()
            .map(|(i, value)| [i as f64, *value])
            .collect();
        list.push(Op::Chart {
            rect: Rect::new(1, 4, state.width - 2, state.height.saturating_sub(7).max(3)),
            datasets: vec![
                Dataset { name: "load".to_string(), points: raw, color: Some(CYAN) },
                Dataset { name: "ema".to_string(), points: smooth, color: Some(VIOLET) },
            ],
            x_bounds: Some([0.0, x_end.max(1.0)]),
            y_bounds: Some([0.0, y_max]),
            border: None,
            fg: None,
            bg: None,
            marker: Marker::Braille,
        });

        list.push(Op::Text {
            rect: Rect::new(1, state.height - 3, state.width - 2, 1),
            spans: vec![
                Span::new(format!("mean {avg:.2}")).fg(DIM),
                Span::new(format!("   sigma {sigma:.2}")).fg(DIM),
                Span::new(format!("   z {z_score:+.1}")).fg(if z_score.abs() >= 2.0 { RED } else { DIM }),
            ],
            align: Align::Center,
            wrap: false,
        });
        list.push(Op::Text {
            rect: Rect::new(1, state.height - 2, state.width - 2, 1),
            spans: vec![
                Span::new(format!("ema {:.2}", ema.last().copied().unwrap_or(0.0))).fg(VIOLET),
                Span::new(format!("   n={}", samples.len())).dim(),
            ],
            align: Align::Center,
            wrap: false,
        });
        list
    },
}
