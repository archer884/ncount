mod app;
mod render;
mod watch;

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Size;
use ratatui::Terminal;

use crate::cli::CommonArgs;
use crate::Result;

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
    io::stdout().execute(EnterAlternateScreen)?;
    install_panic_hook();
    Ok(Terminal::new(CrosstermBackend::new(io::stdout()))?)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
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
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let page_size = page_size(terminal.size()?);
                    handle_key(app, key, &rows, page_size);
                }
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
        Mode::Normal => handle_normal_key(app, key, rows, page_size),
    }
}

fn handle_normal_key(app: &mut App, key: KeyEvent, rows: &[RowData], page_size: usize) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => app.select_next(rows.len()),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
        KeyCode::PageDown => app.select_page_down(page_size, rows.len()),
        KeyCode::PageUp => app.select_page_up(page_size),
        KeyCode::Char('v') | KeyCode::Char(' ') => app.toggle_selected(rows),
        KeyCode::Right => app.expand_selected(rows),
        KeyCode::Left => app.collapse_selected(rows),
        KeyCode::Char('f') | KeyCode::Char('/') => app.enter_filter_mode(),
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
