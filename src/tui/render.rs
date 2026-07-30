use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::app::{App, Mode, RowData};

const WIDTHS: [Constraint; 3] = [Constraint::Fill(1), Constraint::Length(8), Constraint::Length(9)];

/// Apply the dim style used for a pinned-but-unselected row's detail. The
/// selected row's detail passes `dim = false` so it matches the row's intensity.
fn dimmed(line: Line<'static>, dim: bool) -> Line<'static> {
    if dim {
        line.style(Style::new().add_modifier(Modifier::DIM))
    } else {
        line
    }
}

/// A right-aligned numeric cell whose optional second line carries a glyph-led
/// detail value stacked beneath the number (same right edge, so the two line
/// up as the number grows).
fn num_cell(top: u32, detail: Option<(&str, u32)>, dim: bool) -> Cell<'static> {
    let top_line = Line::from(top.to_string()).right_aligned();
    match detail {
        Some((glyph, value)) => {
            let bottom = dimmed(Line::from(format!("{glyph} {value}")).right_aligned(), dim);
            Cell::from(Text::from(vec![top_line, bottom]))
        }
        None => Cell::from(top_line),
    }
}

pub fn draw(frame: &mut Frame, app: &mut App, rows: &[RowData]) {
    let [table_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    let selected = app.table_state.selected();

    let mut running = 0u32;
    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            running += row.paragraphs.total;
            let pinned = app
                .expanded
                .contains(&(row.path.clone(), row.heading.clone()));
            let is_selected = Some(i) == selected;
            build_row(row, running, is_selected, pinned)
        })
        .collect();

    let header = Row::new(vec![
        Cell::from(" §"),
        Cell::from(Line::from("Words").right_aligned()),
        Cell::from(Line::from("Total").right_aligned()),
    ])
    .style(Style::new().add_modifier(Modifier::BOLD));

    let table = Table::new(table_rows, WIDTHS)
        .header(header)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, table_area, &mut app.table_state);
    frame.render_widget(Paragraph::new(footer_text(app, running)), footer_area);
}

fn build_row(row: &RowData, running_total: u32, selected: bool, pinned: bool) -> Row<'static> {
    let indent = "  ".repeat(row.level.saturating_sub(1).max(0) as usize);
    let marker = if pinned { "●" } else { " " };
    let verbose = selected || pinned;
    // Detail on a pinned-but-unselected row recedes; the selected row's detail
    // matches the row's own intensity.
    let dim = !selected;

    let heading_line = Line::from(format!("{marker} {indent}{}", row.heading)).left_aligned();
    // ¶count rides on line 2 of the heading cell, right-aligned so it tucks
    // against the Words column — the "empty column" beneath the heading.
    let heading_cell = if verbose {
        let p_count = dimmed(Line::from(format!("¶ {}", row.paragraphs.count)).right_aligned(), dim);
        Cell::from(Text::from(vec![heading_line, p_count]))
    } else {
        Cell::from(heading_line)
    };

    let words_cell = num_cell(
        row.paragraphs.total,
        verbose.then_some(("⟂", row.paragraphs.average_len())),
        dim,
    );
    let total_cell = num_cell(running_total, verbose.then_some(("▸", row.paragraphs.max)), dim);

    let row = Row::new([heading_cell, words_cell, total_cell]);
    // ratatui's Row height defaults to 1 and truncates extra cell lines, so the
    // 2-line cells above are only shown when we raise it explicitly.
    if verbose {
        row.height(2)
    } else {
        row
    }
}

fn footer_text(app: &App, running_total: u32) -> String {
    match &app.mode {
        Mode::Filter { buffer, .. } => format!("/{buffer}"),
        Mode::Normal => {
            if let Some(status) = &app.status {
                status.clone()
            } else {
                format!(
                    "{running_total} words  |  j/k move  PgUp/PgDn page  v pin  →/← pin/unpin  f // filter  q quit"
                )
            }
        }
    }
}
