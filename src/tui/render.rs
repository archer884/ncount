use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

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
    frame.render_widget(Paragraph::new(footer_text(app, running)), footer_area);
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

fn footer_text(app: &App, running_total: u32) -> String {
    match &app.mode {
        Mode::Filter { buffer, .. } => format!("/{buffer}"),
        Mode::Normal => {
            if let Some(status) = &app.status {
                status.clone()
            } else {
                format!(
                    "{running_total} words  |  j/k move  PgUp/PgDn page  v pin  h/l/←/→ fold  click select/fold  rclick pin  wheel move  f // filter  q quit"
                )
            }
        }
    }
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
}
