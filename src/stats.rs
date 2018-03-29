use lex::{Lexeme, Lexer};
use split_words::SplitWords;
use std::cmp;
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
        let words = s.split_words().count() as u32;
        self.longest_paragraph = cmp::max(words, self.longest_paragraph);
        self.paragraphs += 1;
        self.words += words;
    }
}

pub struct Collector {
    lexer: Lexer,
    sections: Vec<(Option<String>, Stats)>,
    total_words: u32,
    max_heading_width: usize,
}

impl Collector {
    pub fn new() -> Self {
        // We set max_heading_width to 7 here because the minimum heading width (for an empty
        // heading) is the width of the string "unknown"
        Collector {
            lexer: Lexer::new(),
            sections: Vec::new(),
            total_words: 0,
            max_heading_width: 7,
        }
    }

    pub fn push_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
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

                Ok(Lexeme::Heading(heading)) => {
                    self.max_heading_width = cmp::max(heading.len(), self.max_heading_width);

                    match section.take() {
                        // We've just found a heading and we didn't already have one. Any text
                        // that has already appeared should not be stored under this heading name,
                        // so we'll push that under an unknown heading name. This case ought to be
                        // pretty rare.
                        None => {
                            section = Some(format_heading(&heading));
                            self.sections.push((None, stats));
                            stats = Stats::default();
                        }

                        // The usual header case; we push the current heading name and accumulated
                        // stats.
                        Some(previous_section) => {
                            self.sections.push((Some(previous_section), stats));
                            section = Some(heading);
                            stats = Stats::default();
                        }
                    }
                }

                Ok(Lexeme::Paragraph(paragraph)) => stats.apply(&paragraph),
            }
        }

        if !stats.is_empty() {
            self.total_words += stats.words;
            self.sections.push((section, stats));
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

        for (ref heading, ref stats) in &self.sections {
            writeln!(
                f,
                "{:width$}{:5}{:5}{:5}{:4}",
                heading.as_ref().map(AsRef::as_ref).unwrap_or("unknown"),
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
