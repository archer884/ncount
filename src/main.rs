extern crate glob;

mod path;
mod split_words;
mod stats;

use stats::Stats;
use path::PathProvider;

fn main() {
    let mut total_words = 0;
    for path in PathProvider::new() {
        if let Ok(stats) = Stats::from_path(path) {
            total_words += stats.words();
            println!("{}", stats);
        }
    }

    println!("{}", total_words);
}
