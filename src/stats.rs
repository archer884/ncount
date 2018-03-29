use lex::{Lexeme, Lexer};
use split_words::SplitWords;
use std::fmt;
use std::io::{BufReader, Result};
use std::path::Path;

#[derive(Debug, Default)]
pub struct Stats {
    words: u32,
    paragraphs: u32,
    longest_paragraph: u32,
}

impl Stats {
    fn is_empty(&self) -> bool {
        self.words == 0 && self.paragraphs == 0 && self.longest_paragraph == 0
    }

    fn apply(&mut self, s: &str) {
        use std::cmp;

        let words = s.split_words().count() as u32;
        self.longest_paragraph = cmp::max(words, self.longest_paragraph);
        self.paragraphs += 1;
        self.words += words;
    }
}

pub struct Collector {
    lexer: Lexer,
    headings: Vec<(String, Stats)>,
    total_words: u32,
    max_heading_width: usize,
}

impl Collector {
    pub fn new() -> Self {
        Collector {
            lexer: Lexer::new(),
            headings: Vec::new(),
            total_words: 0,
            max_heading_width: 0,
        }
    }

    pub fn push_path<T: AsRef<Path>>(&mut self, path: T) -> Result<()> {
        use std::cmp;
        use std::fs::File;

        fn format_heading(heading: &str) -> String {
            heading.trim_left_matches(|c| c == '#').trim().into()
        }

        let file = File::open(path).map(BufReader::new)?;
        let lexemes = self.lexer.lexemes(file);

        let mut section = None;
        let mut stats = Stats::default();

        for lexeme in lexemes {
            match lexeme {
                Err(e) => return Err(e),

                Ok(Lexeme::Heading(heading)) => match section.take() {
                    None => section = Some(format_heading(&heading)),
                    Some(previous_section) => {
                        self.headings.push((previous_section, stats));
                        section = Some(heading);
                        stats = Stats::default();
                    }
                },

                Ok(Lexeme::Paragraph(paragraph)) => stats.apply(&paragraph),
            }
        }

        if !stats.is_empty() {
            self.max_heading_width = section
                .as_ref()
                .map(|heading| cmp::max(self.max_heading_width, heading.len()))
                .unwrap_or(0);

            self.total_words += stats.words;
            self.headings
                .push((section.unwrap_or_else(|| String::from("unknown")), stats));
        }

        Ok(())
    }
}

impl fmt::Display for Collector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn derive_width(width: usize) -> usize {
            // Leave at least one blank
            let width = width + 1;
            width + width % 4
        }

        fn average_paragraph(stats: &Stats) -> u32 {
            match stats.paragraphs {
                0 => stats.words,
                n => stats.words / n,
            }
        }

        for (ref heading, ref stats) in &self.headings {
            writeln!(
                f,
                "{:width$}{:5}{:5}{:5}{:4}",
                heading,
                stats.words,
                stats.paragraphs,
                average_paragraph(stats),
                stats.longest_paragraph,
                width = derive_width(self.max_heading_width)
            )?;
        }

        writeln!(f, "\nTotal words: {}", self.total_words)
    }
}
