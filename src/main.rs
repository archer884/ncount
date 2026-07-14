mod cli;
mod document;
mod error;
mod filter;
mod fmt;
mod log;
mod tui;

use std::{fs, process};

use cli::{Args, Command, CommonArgs};
use document::DocumentBuilder;
use filter::TextFilter;
use fmt::StatFmt;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() {
    log::init();

    let args = Args::parse();
    let result = match args.command {
        Some(Command::Tui(common)) => tui::run(&common),
        None => run_once(&args.common),
    };

    if let Err(e) = result {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn run_once(args: &CommonArgs) -> Result<()> {
    let filter = TextFilter::new();
    let mut builder = DocumentBuilder::new();

    for file in args.materialize_files() {
        tracing::debug!("path: {}", file.display());
        let text = fs::read_to_string(file)?;
        builder.apply(filter.lex(&text))
    }

    let mut formatter = StatFmt::new(args.verbose());
    if let Some(filter) = args.filter() {
        formatter.add_filter(filter);
    }

    formatter.format(&builder.finalize())?;
    Ok(())
}
