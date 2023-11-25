mod cli;
mod error;
mod log;

use std::process;

use cli::Args;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() {
    log::init();

    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    for file in args.materialize_files() {
        tracing::debug!("path: {}", file.display());
    }

    Ok(())
}
