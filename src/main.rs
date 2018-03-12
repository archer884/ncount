extern crate glob;
extern crate regex;

mod path;
mod split_words;
mod stats;

use path::PathProvider;
use stats::StatsCollector;

fn main() {
    let mut collector = StatsCollector::new();
    for path in PathProvider::new() {
        if let Ok(stats) = collector.from_path(path) {
            println!("{}", stats);
        }
    }

    println!("{}", collector.total_words());
}
