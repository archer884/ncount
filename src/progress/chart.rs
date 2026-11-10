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

use super::diff::Change;

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
/// Gold-star days. Blue against yellow also survives the common
/// (red-green) color-vision deficiencies.
const SUCCESS: Color = Color::Yellow;
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

/// Words added and cut per day as a bar chart: one or more columns per day,
/// added words rising from a zero line and cut words hanging below it. `None`
/// days (before the history starts) are left blank. Assumes `changes` fits
/// `width`, which the caller guarantees by sizing the window from `GUTTER`.
///
/// The two halves are separate charts stacked on one axis: ratatui paints
/// datasets as layers that replace each other a whole cell at a time, so a
/// day with both bars in one chart would lose part of one where they meet.
/// See `split_rows` for how the rows are shared; each half's extreme
/// reaches its edge, though the top is never below the goal, so the goal
/// stays on the chart through a slow week. A day that adds `goal` words or
/// more is a success: the goal is marked on the y scale, and those days'
/// bars get their own color.
pub fn render(start: Date, changes: &[Option<Change>], width: usize, goal: u32) -> Buffer {
    let bar = bar_width(changes.len(), width);
    // Leave a gap between wide bars so neighboring days stay distinct.
    let fill = if bar >= 3 { bar - 1 } else { bar };
    let plot = changes.len() * bar;

    let mut added = Vec::new();
    let mut successes = Vec::new();
    let mut cut = Vec::new();
    for (day, change) in changes.iter().enumerate() {
        let Some(change) = change else { continue };
        let left = day * bar;
        // Braille cells are two dots wide; fill both so bars are solid.
        let dots = 2 * left..2 * (left + fill);
        // A side with no words gets no bar, just the zero line.
        if change.added > 0 {
            let height = f64::from(change.added);
            let points = if change.added >= goal {
                &mut successes
            } else {
                &mut added
            };
            points.extend(dots.clone().map(|dot| (dot as f64, height)));
        }
        if change.cut > 0 {
            let depth = -f64::from(change.cut);
            cut.extend(dots.map(|dot| (dot as f64, depth)));
        }
    }

    // The zero line spans only days with data, so "no history yet" reads
    // differently from "wrote nothing".
    let first = changes.iter().position(Option::is_some).unwrap_or(0) * bar;
    let zero = [
        (2.0 * first as f64, 0.0),
        ((2 * plot).saturating_sub(1) as f64, 0.0),
    ];

    let most_added = changes.iter().flatten().map(|c| c.added).max().unwrap_or(0);
    let most_cut = changes.iter().flatten().map(|c| c.cut).max().unwrap_or(0);
    // The upper half always reaches the goal, so it's never empty.
    let top = most_added.max(goal);
    let (upper_rows, lower_rows) = split_rows(top, most_cut);

    let mut x_labels = vec![Line::from(start.strftime("%b %-d").to_string())];
    if plot >= 40 {
        let middle = start.saturating_add((changes.len() as i64 / 2).days());
        x_labels.push(Line::from(middle.strftime("%b %-d").to_string()));
    }
    x_labels.push(Line::from("today"));
    // In braille dots, two per column, so each maps exactly. Without
    // labels, ratatui draws no x axis, which is how the upper half sits
    // directly on the lower one.
    let x_axis = |labels: Vec<Line<'static>>| {
        Axis::default()
            .style(MUTED)
            .bounds([0.0, (2 * plot).saturating_sub(1).max(1) as f64])
            .labels(labels)
    };
    let bars = |data, color| {
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Bar)
            .style(color)
            .data(data)
    };

    let width = (GUTTER + plot) as u16;
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, PLOT_ROWS + 2));

    // Upper half: the zero line along its bottom edge, gains above, up to
    // the best day or the goal, whichever is higher.
    let upper = Chart::new(vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(MUTED)
            .data(&zero),
        bars(&added, RISE),
        bars(&successes, SUCCESS),
    ])
    .x_axis(x_axis(match lower_rows {
        0 => x_labels.clone(),
        _ => Vec::new(),
    }))
    .y_axis(
        Axis::default()
            .style(MUTED)
            .bounds([0.0, f64::from(top)])
            .labels([label(0), label(top.into())]),
    );
    let axis_rows = if lower_rows == 0 { 2 } else { 0 };
    upper.render(Rect::new(0, 0, width, upper_rows + axis_rows), &mut buffer);
    mark_goal(&mut buffer, upper_rows, top, goal);

    // Lower half: cuts hanging from its top edge, then the x axis.
    if lower_rows > 0 {
        let lower = Chart::new(vec![bars(&cut, FALL)])
            .x_axis(x_axis(x_labels))
            .y_axis(
                Axis::default()
                    .style(MUTED)
                    .bounds([-f64::from(most_cut), 0.0])
                    .labels([label(-i64::from(most_cut)), String::new()]),
            );
        lower.render(Rect::new(0, upper_rows, width, lower_rows + 2), &mut buffer);
    }
    buffer
}

/// Mark the goal on the upper half's y scale with a tick and, unless its
/// row already holds the top or zero label, its own label. Drawn onto the
/// rendered buffer because ratatui only spaces axis labels evenly.
fn mark_goal(buffer: &mut Buffer, rows: u16, top: u32, goal: u32) {
    // The same mapping ratatui's canvas uses: braille, four dots per row.
    let dots = f64::from(rows * 4 - 1);
    let dot = (f64::from(top - goal) * dots / f64::from(top)).round() as u16;
    let row = dot / 4;
    let style = Style::default().fg(SUCCESS);
    let gutter = 0..LABEL_WIDTH as u16;
    // When no day reached the goal, the top label *is* the goal's.
    if top == goal || gutter.clone().all(|x| buffer[(x, row)].symbol() == " ") {
        buffer.set_string(0, row, label(goal.into()), style);
    }
    buffer[(LABEL_WIDTH as u16, row)]
        .set_symbol("┤")
        .set_style(style);
}

/// Plot rows for (gains, cuts). In proportion to the window's most added
/// and most cut, so both halves share one scale, except that gains always
/// keep at least half the chart: a day that purges a whole chapter would
/// otherwise flatten every drafting day to a sliver. Cuts beyond that are
/// scaled down to fit. With no gains, the upper half is the zero line's
/// one row (`render` passes at least the goal, so it never does); with no
/// cuts, there's no lower half at all.
fn split_rows(most_added: u32, most_cut: u32) -> (u16, u16) {
    if most_cut == 0 {
        return (PLOT_ROWS, 0);
    }
    let floor = if most_added == 0 { 1 } else { PLOT_ROWS / 2 };
    let share = f64::from(most_added) / (f64::from(most_added) + f64::from(most_cut));
    let upper = ((f64::from(PLOT_ROWS) * share).round() as u16).clamp(floor, PLOT_ROWS - 1);
    (upper, PLOT_ROWS - upper)
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

    /// A day that only added (positive) or only cut (negative).
    fn net(words: i64) -> Option<Change> {
        let n = words.unsigned_abs() as u32;
        Some(match words {
            0.. => Change { added: n, cut: 0 },
            _ => Change { added: 0, cut: n },
        })
    }

    fn both(added: u32, cut: u32) -> Option<Change> {
        Some(Change { added, cut })
    }

    fn plain(changes: &[Option<Change>], width: usize) -> Vec<String> {
        let buffer = render(date(2026, 3, 1), changes, width, 500);
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
        let buffer = render(date(2026, 3, 1), &[net(5), net(-5)], 80, 500);
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
        let changes: Vec<_> = (0..20).map(|day| net(day * 100 - 500)).collect();
        let lines = plain(&changes, GUTTER + 20);

        assert!(lines[0].starts_with("+1,400"), "{lines:#?}");
        assert!(lines[9].starts_with("  -500"), "{lines:#?}");
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
        let lines = plain(&[None, None, None, None, net(10), net(20)], GUTTER + 18);
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
        let lines = plain(&[net(400), net(0), net(400)], 80);
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
        let lines = plain(&[net(0), net(0)], 80);
        assert!(lines.iter().any(|l| l.starts_with("     0")), "{lines:#?}");
        // The scale still reaches the goal.
        assert!(lines[0].starts_with("  +500┤"), "{lines:#?}");
    }

    #[test]
    fn a_single_day_renders() {
        let lines = plain(&[net(642)], 80);
        assert!(lines[0].starts_with("  +642│"), "{lines:#?}");
        assert!(lines.last().unwrap().ends_with("today"), "{lines:#?}");
    }

    #[test]
    fn a_day_with_both_draws_both_bars_whole() {
        // Drafted a scene, deleted its beat sheet: one bar each way.
        let lines = plain(&[both(0, 0), both(600, 600), both(0, 0)], GUTTER + 3);
        let (upper, _) = split_rows(600, 600);
        let bar = bar_width(3, GUTTER + 3);
        let column = |row: usize, day: usize| lines[row].chars().nth(GUTTER + day * bar).unwrap();

        // Solid from the top of the chart to the bottom of the plot,
        // including the zero line's row, where neither half covers the other.
        for row in 0..PLOT_ROWS as usize {
            assert_eq!(column(row, 1), '⣿', "row {row}: {lines:#?}");
        }
        assert_eq!(column(upper as usize - 1, 0), '⣀', "{lines:#?}");
        assert!(lines[0].starts_with("  +600"), "{lines:#?}");
        assert!(
            lines[upper as usize - 1].starts_with("     0"),
            "{lines:#?}"
        );
        assert!(
            lines[PLOT_ROWS as usize - 1].starts_with("  -600"),
            "{lines:#?}"
        );
        assert_eq!(lines.len(), PLOT_ROWS as usize + 2);
    }

    #[test]
    fn rows_split_by_scale_but_gains_keep_half() {
        assert_eq!(split_rows(500, 0), (PLOT_ROWS, 0));
        assert_eq!(split_rows(900, 100), (9, 1));
        assert_eq!(split_rows(300, 200), (6, 4));
        // A purged chapter doesn't flatten the drafting days.
        assert_eq!(split_rows(500, 9000), (PLOT_ROWS / 2, PLOT_ROWS / 2));
        // Nothing added: the zero line gets one row at the top.
        assert_eq!(split_rows(0, 300), (1, PLOT_ROWS - 1));
    }

    #[test]
    fn only_cuts_still_leave_room_for_the_goal() {
        let lines = plain(&[net(-30), net(-10)], 80);
        let (upper, _) = split_rows(500, 30);
        assert!(lines[0].starts_with("  +500┤"), "{lines:#?}");
        assert!(
            lines[upper as usize - 1].starts_with("     0│"),
            "{lines:#?}"
        );
        assert!(
            lines[PLOT_ROWS as usize - 1].starts_with("   -30"),
            "{lines:#?}"
        );
        assert!(lines.last().unwrap().ends_with("today"), "{lines:#?}");
    }

    #[test]
    fn the_goal_is_marked_at_its_row_on_the_scale() {
        let lines = plain(&[net(2000), net(100)], 80);
        // 500 of 0..=2000, in 40 dots: dot round(1500 * 39 / 2000) = 29.
        assert!(lines[29 / 4].starts_with("  +500┤"), "{lines:#?}");
        assert!(lines[0].starts_with("+2,000│"), "{lines:#?}");
    }

    #[test]
    fn the_goal_is_marked_on_the_upper_half_alone() {
        let lines = plain(&[both(2000, 1000)], 80);
        let (upper, _) = split_rows(2000, 1000);
        assert_eq!(upper, 7);
        // 500 of 0..=2000, in 28 dots: dot round(1500 * 27 / 2000) = 20.
        assert!(lines[20 / 4].starts_with("  +500┤"), "{lines:#?}");
    }

    #[test]
    fn the_goal_stays_on_the_chart_when_no_day_reaches_it() {
        let lines = plain(&[net(100), net(-50)], 80);
        assert!(lines[0].starts_with("  +500┤"), "{lines:#?}");
    }

    #[test]
    fn the_goal_is_configurable() {
        let buffer = render(date(2026, 3, 1), &[net(1500), net(900)], 80, 1000);
        let lines: Vec<String> = to_text(&buffer, false)
            .lines()
            .map(str::to_string)
            .collect();
        // 1000 of 0..=1500, in 40 dots: dot round(500 * 39 / 1500) = 13.
        assert!(lines[13 / 4].starts_with("+1,000┤"), "{lines:#?}");

        let bar = bar_width(2, 80);
        let color = |day: usize| buffer[((GUTTER + day * bar) as u16, PLOT_ROWS - 1)].fg;
        assert_eq!(color(0), SUCCESS);
        assert_eq!(color(1), RISE);
    }

    #[test]
    fn days_reaching_the_goal_get_their_own_color() {
        let changes = [net(499), net(500), net(1000)];
        let buffer = render(date(2026, 3, 1), &changes, 80, 500);
        let bar = bar_width(changes.len(), 80);
        let zero_row = PLOT_ROWS - 1;
        let color = |day: usize| buffer[((GUTTER + day * bar) as u16, zero_row)].fg;

        assert_eq!(color(0), RISE);
        assert_eq!(color(1), SUCCESS);
        assert_eq!(color(2), SUCCESS);
    }

    #[test]
    fn success_counts_words_added_not_net() {
        // A scene drafted over its beat sheet: +600 / -400 nets only +200,
        // but 600 words were written.
        let changes = [both(600, 400), both(300, 0)];
        let buffer = render(date(2026, 3, 1), &changes, 80, 500);
        let (upper, _) = split_rows(600, 400);
        let bar = bar_width(changes.len(), 80);
        let color = |day: usize| buffer[((GUTTER + day * bar) as u16, upper - 1)].fg;

        assert_eq!(color(0), SUCCESS);
        assert_eq!(color(1), RISE);
    }
}
