use std::env;
use std::fmt;
use std::io;
use std::path::PathBuf;

struct Stats {
    path: PathBuf,
    words: u32,
    paragraphs: u32,
    longest_paragraph: u32,
}

impl Stats {
    fn from_path<T: Into<PathBuf>>(path: T) -> io::Result<Self> {
        use std::cmp;
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let path = path.into();
        let file = BufReader::new(File::open(&path)?);

        let mut stats = Stats {
            path,
            words: 0,
            paragraphs: 0,
            longest_paragraph: 0,
        };

        for line in file.lines() {
            if let Ok(line) = line {
                if !line.starts_with('#') && !line.is_empty() {
                    let words = line.split_whitespace().count() as u32;
                    stats.longest_paragraph = cmp::max(words, stats.longest_paragraph);
                    stats.paragraphs += 1;
                    stats.words += words;
                }
            }
        }

        Ok(stats)
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let filename = self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");

        write!(
            f,
            "{}: {}, {} ({})",
            filename, self.words, self.paragraphs, self.longest_paragraph
        )
    }
}

fn main() {
    for path in env::args().skip(1) {
        if let Ok(stats) = Stats::from_path(path) {
            println!("{}", stats);
        }
    }
}
