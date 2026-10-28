use jiff::ToSpan;
use jiff::civil::Date;
use ratatui::backend::IntoCrossterm;
use ratatui::buffer::Buffer;
use ratatui::crossterm::Command;
use ratatui::crossterm::style::{Attribute, ContentStyle, SetAttribute, SetStyle};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::Line;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Widget};

/// Rows for the bars (four braille dots of height each); the x axis and
/// its labels add two more. Deliberately short: a compact chart is more
/// useful than a tall one.
const PLOT_ROWS: u16 = 10;
/// Y labels are padded to this width so the plot's left edge (and so each
/// day's column) is known before rendering.
const LABEL_WIDTH: usize = 6;
/// Columns left of the plot: the y labels and the axis line.
pub const GUTTER: usize = LABEL_WIDTH + 1;
const MAX_BAR_WIDTH: usize = 8;
/// ratatui caps the y labels at a third of the chart's width; any narrower
/// and the gutter shrinks, shifting every day off its column.
const MIN_PLOT_WIDTH: usize = 2 * GUTTER;

const RISE: Color = Color::Cyan;
const FALL: Color = Color::Red;
const MUTED: Color = Color::DarkGray;

/// Columns per day when charting `days` days in `width` columns. A window
/// of only a day or two gets wider bars rather than a too-narrow chart.
pub fn bar_width(days: usize, width: usize) -> usize {
    let days = days.max(1);
    (width.saturating_sub(GUTTER) / days)
        .clamp(1, MAX_BAR_WIDTH)
        .max(MIN_PLOT_WIDTH.div_ceil(days))
}

/// Net words per day as a bar chart: one or more columns per day, gains
/// rising from a zero line and cuts hanging below it. `None` days (before
/// the history starts) are left blank. Assumes `deltas` fits `width`, which
/// the caller guarantees by sizing the window from `GUTTER`. The window's
/// extremes reach the top and bottom.
pub fn render(start: Date, deltas: &[Option<i64>], width: usize) -> Buffer {
    let bar = bar_width(deltas.len(), width);
    // Leave a gap between wide bars so neighboring days stay distinct.
    let fill = if bar >= 3 { bar - 1 } else { bar };
    let plot = deltas.len() * bar;

    let mut rises = Vec::new();
    let mut falls = Vec::new();
    for (day, delta) in deltas.iter().enumerate() {
        // A zero day gets no bar, just the zero line under it.
        let Some(delta) = delta.filter(|&delta| delta != 0) else {
            continue;
        };
        let points = if delta < 0 { &mut falls } else { &mut rises };
        let left = day * bar;
        // Braille cells are two dots wide; fill both so bars are solid.
        let dots = 2 * left..2 * (left + fill);
        points.extend(dots.map(|dot| (dot as f64, delta as f64)));
    }

    // The zero line spans only days with data, so "no history yet" reads
    // differently from "wrote nothing".
    let first = deltas.iter().position(Option::is_some).unwrap_or(0) * bar;
    let zero = [
        (2.0 * first as f64, 0.0),
        ((2 * plot).saturating_sub(1) as f64, 0.0),
    ];

    let lo = deltas.iter().flatten().copied().min().unwrap_or(0).min(0);
    let hi = deltas.iter().flatten().copied().max().unwrap_or(0).max(0);
    // Bounds need some height even when every day was zero; the labels
    // still show only real values.
    let top = if lo == hi { 1 } else { hi };
    let top_label = if lo == hi {
        " ".repeat(LABEL_WIDTH)
    } else {
        label(hi)
    };

    let mut x_labels = vec![Line::from(start.strftime("%b %-d").to_string())];
    if plot >= 40 {
        let middle = start.saturating_add((deltas.len() as i64 / 2).days());
        x_labels.push(Line::from(middle.strftime("%b %-d").to_string()));
    }
    x_labels.push(Line::from("today"));

    let datasets = vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(MUTED)
            .data(&zero),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Bar)
            .style(RISE)
            .data(&rises),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Bar)
            .style(FALL)
            .data(&falls),
    ];
    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .style(MUTED)
                // In braille dots, two per column, so each maps exactly.
                .bounds([0.0, (2 * plot).saturating_sub(1).max(1) as f64])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .style(MUTED)
                .bounds([lo as f64, top as f64])
                .labels([label(lo), top_label]),
        );

    let area = Rect::new(0, 0, (GUTTER + plot) as u16, PLOT_ROWS + 2);
    let mut buffer = Buffer::empty(area);
    chart.render(area, &mut buffer);
    buffer
}

/// The buffer as lines of text, with ANSI styling when `color` is set.
pub fn to_text(buffer: &Buffer, color: bool) -> String {
    let mut out = String::new();
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        let row: Vec<_> = (area.left()..area.right())
            .map(|x| &buffer[(x, y)])
            .collect();
        let end = row
            .iter()
            .rposition(|cell| cell.symbol() != " ")
            .map_or(0, |i| i + 1);
        let mut current: Option<Style> = None;
        for cell in &row[..end] {
            if color && current != Some(cell.style()) {
                let style: ContentStyle = cell.style().into_crossterm();
                let _ = SetAttribute(Attribute::Reset).write_ansi(&mut out);
                let _ = SetStyle(style).write_ansi(&mut out);
                current = Some(cell.style());
            }
            out.push_str(cell.symbol());
        }
        if current.is_some() {
            let _ = SetAttribute(Attribute::Reset).write_ansi(&mut out);
        }
        out.push('\n');
    }
    out
}

/// A y-axis label: signed, compact past four digits, padded to the gutter.
fn label(words: i64) -> String {
    let sign = match words {
        1.. => "+",
        ..0 => "-",
        0 => "",
    };
    let n = words.unsigned_abs();
    let body = match n {
        ..10_000 => grouped(n),
        10_000..999_500 => format!("{}k", (n + 500) / 1000),
        _ => format!("{:.1}M", n as f64 / 1e6),
    };
    format!("{:>LABEL_WIDTH$}", format!("{sign}{body}"))
}

/// `12345` → `12,345`.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn plain(deltas: &[Option<i64>], width: usize) -> Vec<String> {
        let buffer = render(date(2026, 3, 1), deltas, width);
        to_text(&buffer, false)
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn labels_are_signed_compact_and_padded() {
        assert_eq!(label(0), "     0");
        assert_eq!(label(1234), "+1,234");
        assert_eq!(label(-254), "  -254");
        assert_eq!(label(12_345), "  +12k");
        assert_eq!(label(-112_499), " -112k");
        assert_eq!(label(2_400_000), " +2.4M");
    }

    #[test]
    fn grouped_inserts_thousands_separators() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(112_286), "112,286");
    }

    #[test]
    fn rendered_height_is_rows_plus_axis_and_labels() {
        let buffer = render(date(2026, 3, 1), &[Some(5), Some(-5)], 80);
        let lines = to_text(&buffer, false).lines().count();
        assert_eq!(lines, PLOT_ROWS as usize + 2);
    }

    #[test]
    fn bar_width_fills_the_room_within_limits() {
        assert_eq!(bar_width(93, 100), 1);
        assert_eq!(bar_width(31, 100), 3);
        assert_eq!(bar_width(3, 100), MAX_BAR_WIDTH);
        assert_eq!(bar_width(500, 100), 1);
        // Too few days to fill the minimum plot: bars widen past the cap.
        assert_eq!(bar_width(1, 10), MIN_PLOT_WIDTH);
        assert_eq!(bar_width(2, 10), MIN_PLOT_WIDTH / 2);
    }

    #[test]
    fn renders_one_column_per_day_with_extremes_labeled() {
        let deltas: Vec<_> = (0..20).map(|day| Some(day * 10 - 50)).collect();
        let lines = plain(&deltas, GUTTER + 20);

        assert!(lines[0].starts_with("  +140"), "{lines:#?}");
        assert!(lines[9].starts_with("   -50"), "{lines:#?}");
        // The tallest bar is the last day's column; the deepest, the first.
        let column = |line: &str, day: usize| line.chars().nth(GUTTER + day).unwrap();
        assert_ne!(column(&lines[0], 19), ' ', "{lines:#?}");
        assert_ne!(column(&lines[9], 0), ' ', "{lines:#?}");
        assert!(lines.iter().all(|l| l.chars().count() <= GUTTER + 20));
        assert!(
            lines.last().unwrap().trim_start().starts_with("Mar 1"),
            "{lines:#?}"
        );
        assert!(lines.last().unwrap().ends_with("today"), "{lines:#?}");
    }

    #[test]
    fn days_without_history_stay_blank() {
        let lines = plain(&[None, None, None, None, Some(10), Some(20)], GUTTER + 18);
        let plot: Vec<Vec<char>> = lines[..10]
            .iter()
            .map(|l| l.chars().skip(GUTTER).collect())
            .collect();

        // Nothing drawn for the first four days (three columns each): no
        // bars, no zero line.
        for row in &plot {
            assert!(row.iter().take(12).all(|&c| c == ' '), "{lines:#?}");
        }
    }

    #[test]
    fn a_day_without_change_gets_no_bar() {
        let lines = plain(&[Some(10), Some(0), Some(10)], 80);
        let bar = bar_width(3, 80);
        let zero_row = &lines[PLOT_ROWS as usize - 1];
        let cell = |day: usize| zero_row.chars().nth(GUTTER + day * bar).unwrap();

        // The zero line alone is the bottom row of dots; a bar fills more.
        assert_eq!(cell(1), '⣀', "{lines:#?}");
        assert_ne!(cell(0), '⣀', "{lines:#?}");
        assert_ne!(cell(2), '⣀', "{lines:#?}");
    }

    #[test]
    fn all_zero_days_render_without_panicking() {
        let lines = plain(&[Some(0), Some(0)], 80);
        assert!(lines.iter().any(|l| l.starts_with("     0")), "{lines:#?}");
        // No made-up "+1" from the padding that keeps the bounds open.
        assert!(lines[0].starts_with("      │"), "{lines:#?}");
    }

    #[test]
    fn a_single_day_renders() {
        let lines = plain(&[Some(42)], 80);
        assert!(lines[0].starts_with("   +42│"), "{lines:#?}");
        assert!(lines.last().unwrap().ends_with("today"), "{lines:#?}");
    }
}
