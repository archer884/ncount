mod cli;
mod document;
mod error;
mod filter;
mod fmt;
mod log;

use std::{fs, process};

use cli::Args;
use document::DocumentBuilder;
use filter::TagFilter;
use fmt::StatFmt;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() {
    log::init();

    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    let filter = TagFilter::new();
    let mut builder = DocumentBuilder::new();

    for file in args.materialize_files() {
        tracing::debug!("path: {}", file.display());
        let text = fs::read_to_string(file)?;
        builder.apply(filter.filter_text(&text))
    }

    args.filter()
        .map(StatFmt::with_filter)
        .unwrap_or_default()
        .format(&builder.finalize())?;

    Ok(())
}
