mod cli;
mod document;
mod error;
mod filter;
mod fmt;
mod log;

use std::{fs, process};

use cli::Args;
use document::DocumentBuilder;
use filter::TextFilter;
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
    let filter = TextFilter::new();
    let mut builder = DocumentBuilder::new();

    for file in args.materialize_files() {
        tracing::debug!("path: {}", file.display());
        let text = fs::read_to_string(file)?;
        builder.apply(filter.filter_text(&text))
    }

    let mut formatter = StatFmt::new(args.verbose());
    if let Some(filter) = args.filter() {
        formatter.add_filter(filter);
    }

    formatter.format(&builder.finalize())?;
    Ok(())
}
