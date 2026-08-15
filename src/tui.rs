mod app;
mod render;
mod watch;

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Rect, Size};

use crate::Result;
use crate::cli::CommonArgs;

use app::{App, Mode, RowData};
use watch::Watch;

const POLL_INTERVAL: Duration = Duration::from_millis(150);

pub fn run(common: &CommonArgs) -> Result<()> {
    let mut app = App::load(common)?;
    let paths: Vec<_> = app.watched_paths().map(|p| p.to_path_buf()).collect();
    let dirs = app.pattern_dirs();
    let mut watcher = Watch::new(
        paths.iter().map(|p| p.as_path()),
        dirs.iter().map(|p| p.as_path()),
    )?;

    let mut terminal = init_terminal()?;
    let outcome = event_loop(&mut terminal, &mut app, &mut watcher);
    restore_terminal()?;
    outcome
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    io::stdout()
        .execute(EnterAlternateScreen)?
        .execute(EnableMouseCapture)?;
    install_panic_hook();
    Ok(Terminal::new(CrosstermBackend::new(io::stdout()))?)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original(info);
    }));
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    io::stdout()
        .execute(LeaveAlternateScreen)?
        .execute(DisableMouseCapture)?;
    Ok(())
}

/// Visible data rows: full height minus the footer (1) and the table
/// header (1). Drives how far a PgUp/PgDn jump moves the selection.
fn page_size(area: Size) -> usize {
    area.height.saturating_sub(2).max(1) as usize
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    watcher: &mut Watch,
) -> Result<()> {
    loop {
        let rows = app.rows();
        terminal.draw(|frame| render::draw(frame, app, &rows))?;

        if event::poll(POLL_INTERVAL)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let page_size = page_size(terminal.size()?);
                    handle_key(app, key, &rows, page_size);
                }
                Event::Mouse(mouse) => {
                    let table_area = render::main_areas(Rect::from(terminal.size()?))[0];
                    handle_mouse(app, mouse, &rows, table_area);
                }
                _ => {}
            }
        }

        for path in watcher.changed() {
            app.reload(&path);
        }
        if app.sync_patterns() {
            watcher.set_tracked(app.watched_paths().map(|p| p.to_path_buf()));
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent, rows: &[RowData], page_size: usize) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match &app.mode {
        Mode::Filter { .. } => handle_filter_key(app, key),
        // Any key dismisses the shortcuts dialog — including `q` and Esc,
        // which would otherwise quit from Normal mode.
        Mode::Help => app.mode = Mode::Normal,
        Mode::Normal => handle_normal_key(app, key, rows, page_size),
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent, rows: &[RowData], table_area: Rect) {
    // Scrolling works in any mode the same way j/k do: it just nudges the
    // cursor. (Mouse capture must be on for these to arrive at all — without
    // it the terminal instead translates wheel notches into arrow keys,
    // which is why scrolling "already worked" before; enabling capture
    // retires that emulation, so we handle the events ourselves.)
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            app.select_next(rows.len());
            return;
        }
        MouseEventKind::ScrollUp => {
            app.select_prev();
            return;
        }
        _ => {}
    }

    // Button clicks only make sense in Normal mode (filter mode is typing).
    if !matches!(app.mode, Mode::Normal) {
        return;
    }

    let Some(index) = clicked_row(mouse.row, table_area, app.table_state.offset(), rows.len())
    else {
        return;
    };
    // `clicked_row` already bounds `index` against `rows.len()`.
    let row = &rows[index];

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // A direct click overrides any deferred (pinned-exception)
            // selection; `select_index` clears that, like select_next does.
            app.select_index(index);
            // Clicking a parent toggles it open/closed; a leaf just gets
            // selected (mirrors `l` doing nothing on a leaf).
            if row.has_children {
                let key = (row.path.clone(), row.heading.clone());
                if !app.expanded.remove(&key) {
                    app.expanded.insert(key);
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.select_index(index);
            app.toggle_selected(rows);
        }
        _ => {}
    }
}

/// Map an absolute terminal row from a mouse event to the index of the data
/// row it landed on, or `None` if it hit the header, the footer, or empty
/// space below the last row. The header occupies the table area's first line
/// (`table_area.y`); data rows start one line below it; the footer sits just
/// past the area's bottom edge, so `table_area.bottom()` is an exclusive
/// bound. The scroll offset is folded in so a clicked screen row resolves to
/// the right underlying data index regardless of where the viewport is.
fn clicked_row(mouse_row: u16, table_area: Rect, offset: usize, row_count: usize) -> Option<usize> {
    if mouse_row <= table_area.y || mouse_row >= table_area.bottom() {
        return None;
    }
    let index = offset + (mouse_row - table_area.y - 1) as usize;
    (index < row_count).then_some(index)
}

fn handle_normal_key(app: &mut App, key: KeyEvent, rows: &[RowData], page_size: usize) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => app.select_next(rows.len()),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
        KeyCode::PageDown => app.select_page_down(page_size, rows.len()),
        KeyCode::PageUp => app.select_page_up(page_size),
        KeyCode::Char('v') | KeyCode::Char(' ') => app.toggle_selected(rows),
        KeyCode::Right | KeyCode::Char('l') => app.expand_selected(rows),
        KeyCode::Left | KeyCode::Char('h') => app.collapse_selected(rows),
        KeyCode::Char('f') | KeyCode::Char('/') => app.enter_filter_mode(),
        KeyCode::Char('?') => app.mode = Mode::Help,
        _ => {}
    }
}

fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.apply_filter(),
        KeyCode::Esc => app.cancel_filter(),
        KeyCode::Backspace => {
            if let Mode::Filter { buffer, .. } = &mut app.mode {
                buffer.pop();
            }
        }
        KeyCode::Char(c) => {
            if let Mode::Filter { buffer, .. } = &mut app.mode {
                buffer.push(c);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(y: u16, height: u16) -> Rect {
        Rect::new(0, y, 80, height)
    }

    #[test]
    fn clicked_row_hits_the_first_data_line_below_the_header() {
        // Header occupies y=0; the first data row is at y=1.
        assert_eq!(clicked_row(1, table(0, 10), 0, 20), Some(0));
    }

    #[test]
    fn clicked_row_ignores_the_header_line() {
        assert_eq!(clicked_row(0, table(0, 10), 0, 20), None);
    }

    #[test]
    fn clicked_row_ignores_the_footer_line() {
        // height 10 -> bottom() = 10; y=10 is the footer (exclusive bound),
        // and the last real data line (bottom()-1 = 9) still maps.
        assert_eq!(clicked_row(10, table(0, 10), 0, 20), None);
        assert_eq!(clicked_row(9, table(0, 10), 0, 20), Some(8));
    }

    #[test]
    fn clicked_row_folds_in_the_scroll_offset() {
        // Scrolled five down: the top data line on screen is now index 5.
        assert_eq!(clicked_row(1, table(0, 10), 5, 20), Some(5));
    }

    #[test]
    fn clicked_row_ignores_empty_space_below_the_last_row() {
        // Only three data rows exist; the fourth data line is empty space.
        assert_eq!(clicked_row(4, table(0, 10), 0, 3), None);
    }

    #[test]
    fn clicked_row_respects_a_nonzero_origin() {
        // A table not anchored at row 0: header at y=2, data from y=3,
        // bottom() = 12.
        let table = table(2, 10);
        assert_eq!(clicked_row(2, table, 0, 20), None); // header
        assert_eq!(clicked_row(3, table, 0, 20), Some(0)); // first data
        assert_eq!(clicked_row(11, table, 0, 20), Some(8)); // last data
        assert_eq!(clicked_row(12, table, 0, 20), None); // footer
    }
}
