//! Draw-list replay: turn a guest [`DrawList`](xtop_wasm_contract::DrawList)
//! into ratatui widgets on the frame.
//!
//! Every op rect is relative to the widget area and is clipped here, so a
//! guest can never draw outside its own area.

use ratatui::prelude::*;
use ratatui::widgets::{
    Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, LineGauge, Paragraph, Sparkline, Wrap,
};
use xtop_wasm_contract as c;
use xtop_widget_api::glyph::{border_for, marker_for, to_color};
use xtop_widget_api::{ChartCharset, WidgetBorders};

/// Replay `list` inside `area`.
pub fn replay(f: &mut Frame, area: Rect, list: &c::DrawList) {
    for op in &list.ops {
        match op {
            c::Op::Block {
                rect,
                border,
                title,
                fg,
                bg,
            } => {
                let block = block_for(*border, title.as_deref(), *fg, *bg);
                f.render_widget(block, clip(area, rect));
            }
            c::Op::Text {
                rect,
                spans,
                align,
                wrap,
            } => {
                if spans.is_empty() {
                    continue;
                }
                let mut paragraph =
                    Paragraph::new(spans_to_text(spans)).alignment(alignment_for(*align));
                if *wrap {
                    paragraph = paragraph.wrap(Wrap { trim: false });
                }
                f.render_widget(paragraph, clip(area, rect));
            }
            c::Op::Gauge {
                rect,
                ratio,
                label,
                fg,
                bg,
                border,
            } => {
                let mut gauge = Gauge::default()
                    .ratio(ratio.clamp(0.0, 1.0))
                    .gauge_style(style_for(*fg, *bg));
                if let Some(label) = label {
                    gauge = gauge.label(label.clone());
                }
                if let Some(border) = border {
                    gauge = gauge.block(block_for(*border, None, None, None));
                }
                f.render_widget(gauge, clip(area, rect));
            }
            c::Op::Bar {
                rect,
                ratio,
                label,
                fg,
                bg,
                border,
            } => {
                let mut bar = LineGauge::default()
                    .ratio(ratio.clamp(0.0, 1.0))
                    .filled_style(style_for(*fg, *bg));
                if let Some(label) = label {
                    bar = bar.label(label.clone());
                }
                if let Some(border) = border {
                    bar = bar.block(block_for(*border, None, None, None));
                }
                f.render_widget(bar, clip(area, rect));
            }
            c::Op::Sparkline { rect, data, fg, bg } => {
                let data: Vec<u64> = data
                    .iter()
                    .map(|v| if *v > 0.0 { v.round() as u64 } else { 0 })
                    .collect();
                let sparkline = Sparkline::default().data(data).style(style_for(*fg, *bg));
                f.render_widget(sparkline, clip(area, rect));
            }
            c::Op::Chart {
                rect,
                datasets,
                x_bounds,
                y_bounds,
                border,
                fg,
                bg,
                marker,
            } => {
                // The datasets borrow from `series`, so build both before
                // the chart and render within this scope.
                let series: Vec<Vec<(f64, f64)>> = datasets
                    .iter()
                    .map(|d| d.points.iter().map(|p| (p[0], p[1])).collect())
                    .collect();
                let marker = marker_for(charset_for(*marker));
                let datasets: Vec<Dataset> = datasets
                    .iter()
                    .zip(series.iter())
                    .map(|(d, points)| {
                        let mut dataset = Dataset::default()
                            .data(points)
                            .marker(marker)
                            .graph_type(GraphType::Line);
                        if !d.name.is_empty() {
                            dataset = dataset.name(d.name.clone());
                        }
                        if let Some(color) = d.color {
                            dataset = dataset.style(Style::default().fg(to_color(color)));
                        }
                        dataset
                    })
                    .collect();

                let mut chart = Chart::new(datasets);
                if let Some(bounds) = x_bounds {
                    chart = chart.x_axis(Axis::default().bounds(*bounds));
                }
                if let Some(bounds) = y_bounds {
                    chart = chart.y_axis(Axis::default().bounds(*bounds));
                }
                if let Some(border) = border {
                    chart = chart.block(block_for(*border, None, *fg, *bg));
                }
                f.render_widget(chart, clip(area, rect));
            }
        }
    }
}

/// Convert a relative contract rect into an absolute, clipped frame rect.
fn clip(area: Rect, r: &c::Rect) -> Rect {
    let x = area.x.saturating_add(r.x).min(area.right());
    let y = area.y.saturating_add(r.y).min(area.bottom());
    let width = r.width.min(area.right().saturating_sub(x));
    let height = r.height.min(area.bottom().saturating_sub(y));
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn style_for(fg: Option<c::Color>, bg: Option<c::Color>) -> Style {
    let mut style = Style::default();
    if let Some(color) = fg {
        style = style.fg(to_color(color));
    }
    if let Some(color) = bg {
        style = style.bg(to_color(color));
    }
    style
}

fn block_for(
    border: c::Border,
    title: Option<&str>,
    fg: Option<c::Color>,
    bg: Option<c::Color>,
) -> Block<'static> {
    let borders = match border {
        c::Border::Native => WidgetBorders::Native,
        c::Border::Rounded => WidgetBorders::Rounded,
        c::Border::Double => WidgetBorders::Double,
        c::Border::Plain => WidgetBorders::Plain,
        c::Border::Ascii => WidgetBorders::Ascii,
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border_for(borders))
        .style(style_for(fg, bg));
    if let Some(title) = title {
        block = block.title(title.to_string());
    }
    block
}

fn to_span(span: &c::Span) -> Span<'static> {
    let mut style = style_for(span.fg, span.bg);
    if span.bold {
        style = style.bold();
    }
    if span.italic {
        style = style.italic();
    }
    if span.underlined {
        style = style.underlined();
    }
    if span.dim {
        style = style.dim();
    }
    Span::styled(span.text.clone(), style)
}

/// Flatten contract spans into a ratatui [`Text`], splitting on `\n` so a
/// guest can draw a multi-line block with a single text op.
fn spans_to_text(spans: &[c::Span]) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    for span in spans {
        let mut parts = span.text.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                let mut styled = to_span(span);
                styled.content = part.to_string().into();
                if let Some(line) = lines.last_mut() {
                    line.spans.push(styled);
                }
            }
            if parts.peek().is_some() {
                lines.push(Line::default());
            }
        }
    }
    Text::from(lines)
}

fn alignment_for(align: c::Align) -> Alignment {
    match align {
        c::Align::Left => Alignment::Left,
        c::Align::Center => Alignment::Center,
        c::Align::Right => Alignment::Right,
    }
}

fn charset_for(marker: c::Marker) -> ChartCharset {
    match marker {
        c::Marker::Braille => ChartCharset::Braille,
        c::Marker::Dot => ChartCharset::Dot,
        c::Marker::Block => ChartCharset::Block,
        c::Marker::HalfBlock => ChartCharset::HalfBlock,
        c::Marker::Bar => ChartCharset::Bar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(list: &c::DrawList, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                replay(f, area, list);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if let Some(cell) = buffer.cell((x, y)) {
                    out.push_str(cell.symbol());
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn block_renders_border_and_title() {
        let list = c::DrawList {
            ops: vec![c::Op::Block {
                rect: c::Rect::full(20, 4),
                border: c::Border::Rounded,
                title: Some("Hello".into()),
                fg: None,
                bg: None,
            }],
        };
        let buffer = render(&list, 20, 4);
        let text = buffer_text(&buffer);
        assert!(text.contains("Hello"), "title missing in:\n{text}");
        assert!(text.contains('╭'), "rounded border missing in:\n{text}");
    }

    #[test]
    fn text_renders_styled_spans() {
        let list = c::DrawList {
            ops: vec![c::Op::Text {
                rect: c::Rect::full(20, 1),
                spans: vec![
                    c::Span::new("CPU "),
                    c::Span::new("42%").fg([123, 216, 143]).bold(),
                ],
                align: c::Align::Left,
                wrap: false,
            }],
        };
        let buffer = render(&list, 20, 1);
        let text = buffer_text(&buffer);
        assert!(text.contains("CPU 42%"), "text missing in:\n{text}");
    }

    #[test]
    fn text_spans_split_on_newlines() {
        let list = c::DrawList {
            ops: vec![c::Op::Text {
                rect: c::Rect::full(20, 3),
                spans: vec![c::Span::new("one\ntwo")],
                align: c::Align::Left,
                wrap: false,
            }],
        };
        let buffer = render(&list, 20, 3);
        let text = buffer_text(&buffer);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("one"), "first line: {text}");
        assert!(lines[1].starts_with("two"), "second line: {text}");
    }

    #[test]
    fn gauge_and_sparkline_render_inside_clipped_rects() {
        let list = c::DrawList {
            ops: vec![
                c::Op::Gauge {
                    rect: c::Rect::new(0, 0, 10, 1),
                    ratio: 1.0,
                    label: Some("99%".into()),
                    fg: Some([123, 216, 143]),
                    bg: None,
                    border: None,
                },
                c::Op::Sparkline {
                    rect: c::Rect::new(0, 2, 10, 1),
                    data: vec![1.0, 2.0, 3.0, 4.0],
                    fg: None,
                    bg: None,
                },
            ],
        };
        let buffer = render(&list, 10, 3);
        let text = buffer_text(&buffer);
        assert!(text.contains("99%"), "gauge label missing in:\n{text}");
        // Sparkline row must have at least one bar glyph.
        assert!(
            text.contains('▁') || text.contains('▂') || text.contains('▃') || text.contains('█'),
            "sparkline bars missing in:\n{text}"
        );
    }

    #[test]
    fn oversized_rects_are_clipped_to_the_widget_area() {
        let list = c::DrawList {
            ops: vec![c::Op::Block {
                rect: c::Rect::new(5, 5, 100, 100),
                border: c::Border::Plain,
                title: None,
                fg: None,
                bg: None,
            }],
        };
        // Must not panic and must draw the visible part only.
        let buffer = render(&list, 10, 10);
        let text = buffer_text(&buffer);
        assert!(text.contains('+'), "ascii border missing in:\n{text}");
    }

    #[test]
    fn chart_renders_a_dataset_line() {
        let list = c::DrawList {
            ops: vec![c::Op::Chart {
                rect: c::Rect::full(20, 8),
                datasets: vec![c::Dataset {
                    name: "cpu".into(),
                    points: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]],
                    color: Some([252, 97, 141]),
                }],
                x_bounds: Some([0.0, 2.0]),
                y_bounds: Some([0.0, 2.0]),
                border: Some(c::Border::Native),
                fg: None,
                bg: None,
                marker: c::Marker::Braille,
            }],
        };
        let buffer = render(&list, 20, 8);
        let text = buffer_text(&buffer);
        assert!(
            text.contains('⡀') || text.contains('⣀') || text.contains('⠉') || text.contains('⠤'),
            "braille line missing in:\n{text}"
        );
    }
}
