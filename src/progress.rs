mod chart;
mod diff;
mod history;

use std::io::{self, IsTerminal, Write};

use jiff::ToSpan;
use owo_colors::OwoColorize;

use crate::Result;
use crate::cli::CommonArgs;
use crate::error::Error;

use chart::grouped;
use diff::Change;
use history::{Clock, Day, History};

/// The window when stdout isn't a terminal and `--days` wasn't given.
const FALLBACK_DAYS: usize = 30;

/// `ncount -p`: a static chart of words added and cut per day, read from the
/// charted files' git history, with today read from the working tree.
pub fn run(args: &CommonArgs) -> Result<()> {
    if args.reads_stdin() {
        return Err(Error::NoPaths);
    }

    let terminal = io::stdout()
        .is_terminal()
        .then(|| ratatui::crossterm::terminal::size().ok())
        .flatten()
        .map(|(width, _)| width as usize);
    let (width, days) = match (terminal, args.days().map(|days| days as usize)) {
        (Some(width), Some(days)) => {
            let room = width.saturating_sub(chart::GUTTER).max(1);
            if days > room {
                warn(&format!(
                    "only {room} days fit the terminal; charting those"
                ));
            }
            (width, days.min(room))
        }
        (Some(width), None) => (width, width.saturating_sub(chart::GUTTER).max(1)),
        // Piped: there's no width to fit, so chart exactly what was asked.
        (None, Some(days)) => (chart::GUTTER + days, days),
        (None, None) => (chart::GUTTER + FALLBACK_DAYS, FALLBACK_DAYS),
    };

    let clock = Clock::system();
    let today = clock.today();
    // One extra day: the baseline the first charted day's change is from.
    let mut baseline = today.saturating_sub((days as i64).days());
    let mut history = History::open(args.paths(), args.filter())?;
    let mut days = history.daily(&clock, baseline, today)?;
    if days.iter().flatten().all(|day| day.tally.files == 0) {
        return Err(Error::NothingMatched);
    }

    // Same fallback as the table and the TUI: a filter that matches nothing
    // (on any day) warns and charts everything.
    let section = match args.filter() {
        Some(_) if days.iter().flatten().any(|day| day.tally.section.is_some()) => true,
        Some(filter) => {
            warn(&format!(
                "no heading matching {filter:?} found; showing everything"
            ));
            false
        }
        None => false,
    };

    // Sized to fit by default, but no wider than the history itself: start
    // at the first day with data. (An explicit `--days` keeps its blanks.)
    if args.days().is_none() {
        let first = days.iter().position(Option::is_some).unwrap_or(0);
        days.drain(..first);
        baseline = baseline.saturating_add((first as i64).days());
    }

    let words = |day: &Day| match section {
        true => day.tally.section.unwrap_or(0),
        false => day.tally.words,
    };
    let now = days.last().and_then(Option::as_ref).map_or(0, words);
    let start = baseline.saturating_add(1.day());
    let changes: Vec<Option<Change>> = days
        .windows(2)
        .zip(start.series(1.day()))
        .map(|(pair, date)| {
            let change = match pair {
                [Some(before), Some(after)] => Some(history.change(before, after, section)),
                _ => None,
            };
            tracing::debug!(%date, ?change);
            change
        })
        .collect();

    let mut out = io::stdout().lock();
    let name = match (section, args.filter()) {
        (true, Some(filter)) => format!("{filter}: "),
        _ => String::new(),
    };
    if changes.is_empty() {
        writeln!(
            out,
            "{name}{} words; no earlier history yet",
            grouped(now.into())
        )?;
        return Ok(());
    }

    let buffer = chart::render(start, &changes, width);
    write!(out, "{}", chart::to_text(&buffer, terminal.is_some()))?;

    let mut total = Change::default();
    for change in changes.iter().flatten() {
        total += *change;
    }
    let mut summary = format!(
        "{name}{} words · +{} -{} (net {}) over {} days",
        grouped(now.into()),
        grouped(total.added.into()),
        grouped(total.cut.into()),
        signed(total.net()),
        changes.len(),
    );
    let best = changes
        .iter()
        .enumerate()
        .filter_map(|(i, change)| change.map(|change| (change.added, i)))
        .max();
    if let Some((added, i)) = best.filter(|&(added, _)| added > 0) {
        let day = start.saturating_add((i as i64).days());
        summary += &format!(
            " · best day +{} ({})",
            grouped(added.into()),
            day.strftime("%b %-d")
        );
    }
    writeln!(out, "{summary}")?;
    Ok(())
}

fn signed(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "+" };
    format!("{sign}{}", grouped(n.unsigned_abs()))
}

fn warn(message: &str) {
    eprintln!("{}", format!("warning: {message}").yellow());
}
