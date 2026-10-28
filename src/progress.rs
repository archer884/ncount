mod chart;
mod history;

use std::io::{self, IsTerminal, Write};

use jiff::ToSpan;
use owo_colors::OwoColorize;

use crate::Result;
use crate::cli::CommonArgs;
use crate::error::Error;

use chart::grouped;
use history::{Clock, History, Tally};

/// The window when stdout isn't a terminal and `--days` wasn't given.
const FALLBACK_DAYS: usize = 30;

/// `ncount -p`: a static chart of net words written per day, read from the
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
    let tallies = history.daily(&clock, baseline, today)?;
    if tallies.iter().flatten().all(|tally| tally.files == 0) {
        return Err(Error::NothingMatched);
    }

    // Same fallback as the table and the TUI: a filter that matches nothing
    // (on any day) warns and charts everything.
    let section = match args.filter() {
        Some(_)
            if tallies
                .iter()
                .flatten()
                .any(|tally| tally.section.is_some()) =>
        {
            true
        }
        Some(filter) => {
            warn(&format!(
                "no heading matching {filter:?} found; showing everything"
            ));
            false
        }
        None => false,
    };
    let words = |tally: &Tally| match section {
        true => tally.section.unwrap_or(0),
        false => tally.words,
    };
    let mut totals: Vec<Option<u32>> = tallies.iter().map(|t| t.as_ref().map(words)).collect();

    // Sized to fit by default, but no wider than the history itself: start
    // at the first day with data. (An explicit `--days` keeps its blanks.)
    if args.days().is_none() {
        let first = totals.iter().position(Option::is_some).unwrap_or(0);
        totals.drain(..first);
        baseline = baseline.saturating_add((first as i64).days());
    }

    let now = totals.last().copied().flatten().unwrap_or(0);
    let deltas: Vec<Option<i64>> = totals
        .windows(2)
        .map(|pair| match (pair[0], pair[1]) {
            (Some(before), Some(after)) => Some(i64::from(after) - i64::from(before)),
            _ => None,
        })
        .collect();

    let mut out = io::stdout().lock();
    let name = match (section, args.filter()) {
        (true, Some(filter)) => format!("{filter}: "),
        _ => String::new(),
    };
    if deltas.is_empty() {
        writeln!(
            out,
            "{name}{} words; no earlier history yet",
            grouped(now.into())
        )?;
        return Ok(());
    }

    let start = baseline.saturating_add(1.day());
    let buffer = chart::render(start, &deltas, width);
    write!(out, "{}", chart::to_text(&buffer, terminal.is_some()))?;

    let first = totals.iter().flatten().next().copied().unwrap_or(0);
    let mut summary = format!(
        "{name}{} words · {} over {} days",
        grouped(now.into()),
        signed(i64::from(now) - i64::from(first)),
        deltas.len(),
    );
    let best = deltas
        .iter()
        .enumerate()
        .filter_map(|(i, delta)| delta.map(|delta| (delta, i)))
        .max();
    if let Some((delta, i)) = best.filter(|&(delta, _)| delta > 0) {
        let day = start.saturating_add((i as i64).days());
        summary += &format!(" · best day {} ({})", signed(delta), day.strftime("%b %-d"));
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
