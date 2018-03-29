extern crate glob;
extern crate regex;

mod lex;
mod path;
mod split_words;
mod stats;

use path::PathProvider;
use stats::Collector;
use std::process;

fn main() {
    let mut collector = Collector::new();

    for path in PathProvider::new() {
        if let Err(e) = collector.push_path(&path) {
            eprintln!("Failed to load path:\n  {}\n  {}", path.display(), e);
            process::exit(1);
        }
    }

    println!("{}", collector);
}
