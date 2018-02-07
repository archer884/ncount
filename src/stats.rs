use std::fmt;
use std::io;

pub struct Stats {
    path: String,
    words: u32,
    paragraphs: u32,
    longest_paragraph: u32,
}

impl Stats {
    pub fn from_path<T: Into<String>>(path: T) -> io::Result<Self> {
        use split_words::SplitWords;
        use std::cmp;
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        // This works to ignore comments because of the way my comments are usually formatted:
        //
        // <!--
        //
        //     <comment>
        //     <text>
        //     <here>
        //
        // -->
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
                if is_valid_line(&line) {
                    let words = line.split_words().count() as u32;
                    stats.longest_paragraph = cmp::max(words, stats.longest_paragraph);
                    stats.paragraphs += 1;
                    stats.words += words;
                }
            }
        }

        Ok(stats)
    }

    pub fn words(&self) -> u32 {
        self.words
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
            "{}\t{}\t{}\t({})",
            filename, self.words, self.paragraphs, self.longest_paragraph
        )
    }
}
