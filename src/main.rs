extern crate glob;
extern crate regex;

mod lex;
mod path;
mod split;
mod stats;

use path::PathProvider;
use stats::Collector;
use std::env;
use std::process;

static VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    version(&args);
    run(&args);
}

/// Run program.
fn run<T: AsRef<str>>(args: &[T]) {
    let mut collector = Collector::new();
    for path in PathProvider::new(args) {
        if let Err(e) = collector.push_path(&path) {
            eprintln!("Failed to load path:\n  {}\n  {}", path.display(), e);
            process::exit(1);
        }
    }
    println!("{}", collector);
}

/// If version requested, print version number and exit.
fn version<T: AsRef<str>>(args: &[T]) {
    for arg in args {
        match arg.as_ref() {
            "-v" | "--version" => {
                println!("ncount {}", VERSION);
                process::exit(0);
            }
            _ => {}
        }
    }
}
