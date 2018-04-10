extern crate glob;
extern crate regex;

mod lex;
mod path;
mod split;
mod stats;

use path::PathProvider;
use stats::Collector;
use std::process;

static VERSION: &str = "0.1.10";

fn main() {
    version();

    let mut collector = Collector::new();

    for path in PathProvider::new() {
        if let Err(e) = collector.push_path(&path) {
            eprintln!("Failed to load path:\n  {}\n  {}", path.display(), e);
            process::exit(1);
        }
    }

    println!("{}", collector);
}

/// If version requested, prints version number and exits.
fn version() {
    use std::env;

    for arg in env::args() {
        match arg.as_ref() {
            "-v" | "--version" => {
                println!("ncount {}", VERSION);
                process::exit(0);
            }
            _ => { }
        }
    }
}
