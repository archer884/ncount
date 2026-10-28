mod cli;
mod document;
mod error;
mod filter;
mod fmt;
mod log;
#[cfg(feature = "progress")]
mod progress;
mod tui;

use std::{
    fs,
    io::{self, IsTerminal, Read},
    process,
};

use cli::{Args, CommonArgs};
use document::DocumentBuilder;
use filter::TextFilter;
use fmt::StatFmt;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() {
    log::init();

    let args = Args::parse();
    let result = match () {
        _ if args.common.watch() => tui::run(&args.common),
        #[cfg(feature = "progress")]
        _ if args.common.progress() => progress::run(&args.common),
        _ => run_once(&args.common),
    };

    if let Err(e) = result {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn run_once(args: &CommonArgs) -> Result<()> {
    let filter = TextFilter::new();
    let mut builder = DocumentBuilder::new();

    if args.reads_stdin() {
        // Bare `ncount` at a prompt would otherwise sit waiting for EOF.
        let stdin = io::stdin();
        if stdin.is_terminal() {
            return Err(error::Error::NoInput);
        }
        let mut text = String::new();
        stdin.lock().read_to_string(&mut text)?;
        builder.apply(filter.lex(&text));
    } else {
        for file in args.materialize_files()? {
            tracing::debug!("path: {}", file.display());
            let text = fs::read_to_string(file)?;
            builder.apply(filter.lex(&text))
        }
    }

    let mut formatter = StatFmt::new(args.verbose());
    if let Some(filter) = args.filter() {
        formatter.add_filter(filter);
    }

    formatter.format(&builder.finalize())?;
    Ok(())
}
