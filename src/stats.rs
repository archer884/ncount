use regex::Regex;
use std::fmt::{self, Display};
use std::io;
use std::path::Path;

pub struct StatsCollector {
    comments: Regex,
    total_words: u32,
}

impl StatsCollector {
    pub fn new() -> Self {
        Self {
            comments: Regex::new(r#"<[^<]*>"#).expect("Failed to initialize comment pattern"),
            total_words: 0,
        }
    }

    pub fn from_path<T: AsRef<Path>>(&mut self, path: T) -> io::Result<Stats> {
        use std::fs::File;
        use std::io::{BufReader, Read};
        
        let buffer = {
            let mut buf = String::new();
            let _ = File::open(&path).map(BufReader::new)?.read_to_string(&mut buf);
            buf
        };

        let buffer = self.comments.replace(&buffer, "");
        let stats = Stats::from_buffer(path.as_ref().display(), &buffer);

        self.total_words += stats.words;
        Ok(stats)
    }

    pub fn total_words(&self) -> u32 {
        self.total_words
    }
}

pub struct Stats {
    path: String,
    words: u32,
    paragraphs: u32,
    longest_paragraph: u32,
}

impl Stats {
    pub fn from_buffer<T: Display>(name: T, s: &str) -> Self {
        use split_words::SplitWords;
        use std::cmp;
        
        // Ignore anything that's not text. Text can start with these legal characters.
        fn is_valid_line(s: &str) -> bool {
            !s.is_empty() && s.starts_with(|c| {
                c == '"'        // Dialog
                || c == '.'     // Ellipsis
                || c == '*'     // Italics
                || {
                    let c = (c as u8) & !32;
                    c >= b'A' && c <= b'Z'
                }
            })
        }

        let mut stats = Stats {
            path: format!("{}", name),
            words: 0,
            paragraphs: 0,
            longest_paragraph: 0,
        };

        for line in s.lines() {
            if is_valid_line(line) {
                let words = line.split_words().count() as u32;
                stats.longest_paragraph = cmp::max(words, stats.longest_paragraph);
                stats.paragraphs += 1;
                stats.words += words;
            }
        }

        stats
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::path::Path;

        let path: &Path = self.path.as_ref();
        let filename = path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");

        write!(
            f,
            "{}\t{}\t{}\t({} / {})",
            filename,
            self.words,
            self.paragraphs,
            average(self.words, self.paragraphs),
            self.longest_paragraph
        )
    }
}

fn average(left: u32, right: u32) -> u32 {
    match right {
        0 => 0,
        _ => left / right,
    }
}
