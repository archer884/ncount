use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::app::{App, Mode, RowData};

/// Right-aligns numeric columns; the heading column stays left-aligned.
fn right(s: impl Into<String>) -> Cell<'static> {
    Cell::from(Line::from(s.into()).right_aligned())
}

const COMPACT_WIDTHS: [Constraint; 3] = [
    Constraint::Fill(1),
    Constraint::Length(8),
    Constraint::Length(9),
];

const VERBOSE_WIDTHS: [Constraint; 6] = [
    Constraint::Fill(1),
    Constraint::Length(8),
    Constraint::Length(6),
    Constraint::Length(7),
    Constraint::Length(8),
    Constraint::Length(9),
];

pub fn draw(frame: &mut Frame, app: &mut App, rows: &[RowData]) {
    let [table_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    let selected = app.table_state.selected();
    let show_detail = rows.iter().enumerate().any(|(i, row)| {
        Some(i) == selected
            || app
                .expanded
                .contains(&(row.file_index, row.heading.clone()))
    });

    let mut running = 0u32;
    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            running += row.paragraphs.total;
            let pinned = app
                .expanded
                .contains(&(row.file_index, row.heading.clone()));
            let verbose = Some(i) == selected || pinned;
            build_row(row, running, verbose, pinned, show_detail)
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
        Table::new(table_rows, VERBOSE_WIDTHS)
    } else {
        Table::new(table_rows, COMPACT_WIDTHS)
    }
    .header(header)
    .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, table_area, &mut app.table_state);
    frame.render_widget(Paragraph::new(footer_text(app, running)), footer_area);
}

fn build_row(
    row: &RowData,
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
            right(row.paragraphs.total.to_string()),
            right(running_total.to_string()),
        ]);
    }

    let (count, avg, max) = if verbose {
        (
            row.paragraphs.count.to_string(),
            row.paragraphs.average_len().to_string(),
            row.paragraphs.max.to_string(),
        )
    } else {
        (String::new(), String::new(), String::new())
    };

    Row::new([
        Cell::from(heading),
        right(count),
        right(avg),
        right(max),
        right(row.paragraphs.total.to_string()),
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
                    "{running_total} words  |  j/k move  PgUp/PgDn page  v pin  →/← pin/unpin  f // filter  q quit"
                )
            }
        }
    }
}
