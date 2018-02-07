mod split_words;
mod stats;

use stats::Stats;
use std::env;

fn main() {
    let mut total_words = 0;
    for path in env::args().skip(1) {
        if let Ok(stats) = Stats::from_path(path) {
            total_words += stats.words();
            println!("{}", stats);
        }
    }

    println!("{}", total_words);
}
