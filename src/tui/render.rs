use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Clear, Paragraph, Row, Table};

use crate::document::Paragraphs;

use super::app::{App, Mode, RowData};

/// Right-aligns numeric columns; the heading column stays left-aligned.
fn right(s: impl Into<String>) -> Cell<'static> {
    Cell::from(Line::from(s.into()).right_aligned())
}

const COMPACT_WIDTHS: [Constraint; 3] = [
    Constraint::Fill(1),
    Constraint::Length(WORDS_WIDTH),
    Constraint::Length(TOTAL_WIDTH),
];

const COLUMN_SPACING: u16 = 1;
const COUNT_WIDTH: u16 = 8;
const AVG_WIDTH: u16 = 6;
const LONG_WIDTH: u16 = 7;
const WORDS_WIDTH: u16 = 8;
const TOTAL_WIDTH: u16 = 9;

const VERBOSE_WIDTHS: [Constraint; 6] = [
    Constraint::Fill(1),
    Constraint::Length(COUNT_WIDTH),
    Constraint::Length(AVG_WIDTH),
    Constraint::Length(LONG_WIDTH),
    Constraint::Length(WORDS_WIDTH),
    Constraint::Length(TOTAL_WIDTH),
];

/// `[table_area, footer_area]` for the app's vertical split (everything but
/// the bottom 1-line footer). Single-sourced here so the event loop's
/// click-to-row mapping can't drift from how `draw` lays things out.
pub(crate) fn main_areas(area: Rect) -> [Rect; 2] {
    Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area)
}

pub fn draw(frame: &mut Frame, app: &mut App, rows: &[RowData]) {
    let [table_area, footer_area] = main_areas(frame.area());

    let selected = app.table_state.selected();
    let show_detail = rows.iter().enumerate().any(|(i, row)| {
        Some(i) == selected
            || app
                .pinned
                .contains(&(row.path.clone(), row.heading.clone()))
    });

    let mut running = 0u32;
    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let pinned = app
                .pinned
                .contains(&(row.path.clone(), row.heading.clone()));
            let verbose = Some(i) == selected || pinned;
            let unfolded = app
                .expanded
                .contains(&(row.path.clone(), row.heading.clone()));
            // A folded parent speaks for its whole subtree: its children are
            // hidden, so both the Words value and the detail columns (Count,
            // Avg, Long) reflect every paragraph under it. Unfolded, the row
            // shows only its own direct paragraphs — the children carry their
            // own rows. This mirrors the existing Words logic exactly.
            let detail = if row.has_children && !unfolded {
                row.subtree_paragraphs
            } else {
                row.paragraphs
            };
            let words = detail.total;
            if !row.pinned_exception {
                running += words;
            }
            build_row(row, detail, words, running, verbose, pinned, show_detail)
        })
        .collect();

    let header = if show_detail {
        Row::new([
            Cell::from(" §"),
            right("Count¶"),
            right("Avg¶"),
            right("Long¶"),
            right("Words"),
            right("Total"),
        ])
    } else {
        Row::new(vec![Cell::from(" §"), right("Words"), right("Total")])
    }
    .style(Style::new().add_modifier(Modifier::BOLD));

    let table = if show_detail {
        Table::new(table_rows, VERBOSE_WIDTHS).column_spacing(COLUMN_SPACING)
    } else {
        Table::new(table_rows, COMPACT_WIDTHS).column_spacing(COLUMN_SPACING)
    }
    .header(header)
    .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, table_area, &mut app.table_state);
    if show_detail {
        render_compact_headings(frame, table_area, app, rows, selected);
    }
    render_footer(frame, footer_area, app, running);

    if matches!(app.mode, Mode::Help) {
        let area = frame.area();
        render_help(frame, area);
    }
}

/// The verbose table keeps its numeric columns aligned globally, but compact
/// rows have no values in Count/Avg/Long. Paint those rows' headings across
/// the otherwise-empty columns, stopping before Words, so their titles get
/// the space they would have had in the compact table.
fn render_compact_headings(
    frame: &mut Frame,
    table_area: Rect,
    app: &App,
    rows: &[RowData],
    selected: Option<usize>,
) {
    let heading_width = verbose_heading_span_width(table_area.width);
    if heading_width == 0 {
        return;
    }

    let offset = app.table_state.offset();
    let first_row_y = table_area.y.saturating_add(1);
    let visible_height = table_area.height.saturating_sub(1);
    let buffer = frame.buffer_mut();

    for (index, row) in rows.iter().enumerate().skip(offset) {
        let row_offset = index - offset;
        if row_offset >= visible_height as usize {
            break;
        }

        let pinned = app
            .pinned
            .contains(&(row.path.clone(), row.heading.clone()));
        if Some(index) == selected || pinned {
            continue;
        }

        let indent = "  ".repeat(row.level.saturating_sub(1).max(0) as usize);
        let heading = format!("  {indent}{}", row.heading);
        let y = first_row_y + row_offset as u16;
        let style = Style::default();
        for x in table_area.x..table_area.x + heading_width {
            buffer[(x, y)].set_symbol(" ").set_style(style);
        }
        buffer.set_stringn(table_area.x, y, heading, heading_width as usize, style);
    }
}

/// Width from the left edge of a verbose table to the start of its Words
/// column, including the blank detail columns and their spacing.
fn verbose_heading_span_width(table_width: u16) -> u16 {
    table_width.saturating_sub(WORDS_WIDTH + COLUMN_SPACING + TOTAL_WIDTH)
}

fn build_row(
    row: &RowData,
    detail: Paragraphs,
    words: u32,
    running_total: u32,
    verbose: bool,
    pinned: bool,
    show_detail: bool,
) -> Row<'static> {
    let indent = "  ".repeat(row.level.saturating_sub(1).max(0) as usize);
    let marker = if pinned { "●" } else { " " };
    let heading = format!("{marker} {indent}{}", row.heading);

    if !show_detail {
        return Row::new([
            Cell::from(heading),
            right(words.to_string()),
            right(running_total.to_string()),
        ]);
    }

    let (count, avg, max) = if verbose {
        (
            detail.count.to_string(),
            detail.average_len().to_string(),
            detail.max.to_string(),
        )
    } else {
        (String::new(), String::new(), String::new())
    };

    Row::new([
        Cell::from(heading),
        right(count),
        right(avg),
        right(max),
        right(words.to_string()),
        right(running_total.to_string()),
    ])
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App, running_total: u32) {
    match &app.mode {
        Mode::Filter { buffer, .. } => {
            frame.render_widget(Paragraph::new(format!("/{buffer}")), area);
        }
        Mode::Normal | Mode::Help => {
            if let Some(status) = &app.status {
                frame.render_widget(Paragraph::new(status.clone()), area);
            } else {
                let [left, right] =
                    Layout::horizontal([Constraint::Min(0), Constraint::Min(0)]).areas(area);
                frame.render_widget(Paragraph::new(format!("{running_total} words")), left);
                frame.render_widget(Paragraph::new("? help").right_aligned(), right);
            }
        }
    }
}

const HELP_KEY_WIDTH: usize = 26;

fn help_group(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::new().add_modifier(Modifier::BOLD),
    ))
}

fn help_entry(key: &'static str, desc: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::from("  "),
        Span::styled(
            format!("{key:<HELP_KEY_WIDTH$}"),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::from(desc),
    ])
}

/// The shortcuts dialog's lines, as data so `help_area` can measure them
/// and the whole catalog stays in one place.
fn help_lines() -> Vec<Line<'static>> {
    vec![
        help_group("Moving"),
        help_entry("j / ↓ / wheel", "move to the next row"),
        help_entry("k / ↑ / wheel", "move to the previous row"),
        help_entry("PgDn", "scroll down a page"),
        help_entry("PgUp", "scroll up a page"),
        Line::default(),
        help_group("Folding & pinning"),
        help_entry("l / →", "unfold the selected section"),
        help_entry("h / ←", "fold; a leaf folds its parent"),
        help_entry("click", "select a row; on a parent, fold/unfold"),
        help_entry(
            "v / Space / right-click",
            "pin or unpin the selected section",
        ),
        Line::default(),
        help_group("Filtering"),
        help_entry("f / /", "filter rows by heading"),
        help_entry("Enter", "apply the filter"),
        help_entry("Esc", "cancel the filter"),
        Line::default(),
        help_group("General"),
        help_entry("?", "show this help"),
        help_entry("q / Esc / Ctrl-C", "quit"),
    ]
}

/// A centered rect just big enough for the dialog's content (plus borders),
/// clamped to the available area so small terminals crop instead of panic.
fn help_area(area: Rect, lines: &[Line<'_>]) -> Rect {
    let content_width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    // borders (2) plus one column of right padding so the longest line
    // doesn't sit flush against the border
    let width = content_width.saturating_add(3).min(area.width);
    let height = (lines.len() as u16).saturating_add(2).min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

fn render_help(frame: &mut Frame, area: Rect) {
    let lines = help_lines();
    let dialog = help_area(area, &lines);
    frame.render_widget(Clear, dialog);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Shortcuts ")),
        dialog,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_heading_span_stops_before_words_and_total() {
        let width = 60;
        let heading = verbose_heading_span_width(width);
        assert_eq!(heading, 42);
        assert_eq!(heading + WORDS_WIDTH + COLUMN_SPACING + TOTAL_WIDTH, width);
    }

    #[test]
    fn help_area_fits_content_plus_borders_and_centers() {
        let lines = help_lines();
        let content_width = lines.iter().map(Line::width).max().unwrap() as u16;

        let area = Rect::new(0, 0, 200, 60);
        let dialog = help_area(area, &lines);

        assert_eq!(dialog.width, content_width + 3);
        assert_eq!(dialog.height, lines.len() as u16 + 2);
        assert_eq!(dialog.x, (area.width - dialog.width) / 2);
        assert_eq!(dialog.y, (area.height - dialog.height) / 2);
    }

    #[test]
    fn help_area_clamps_to_a_small_terminal() {
        let lines = help_lines();
        let area = Rect::new(0, 0, 20, 10);
        let dialog = help_area(area, &lines);

        assert_eq!(dialog, area);
    }
}
