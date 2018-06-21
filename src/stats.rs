use lex::{Lexeme, Lexer};
use std::cmp;
use std::fmt;
use std::io::{BufRead, Result};
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

    fn push_paragraph(&mut self, paragraph: u32) {
        self.longest_paragraph = cmp::max(paragraph, self.longest_paragraph);
        self.paragraphs += 1;
        self.words += paragraph;
    }
}

pub struct Collector {
    lexer: Lexer,
    sections: Vec<(Option<String>, Stats)>,
    total_words: u32,
    heading_width: usize,
}

impl fmt::Debug for Collector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut d = f.debug_struct("Collector");
        d.field("sections", &self.sections);
        d.field("total_words", &self.total_words);
        d.field("heading_width", &self.heading_width);
        d.finish()
    }
}

impl Collector {
    pub fn new() -> Self {
        // We set heading_width to 7 here because the minimum heading width (for an empty
        // heading) is the width of the string "unknown"
        Collector {
            lexer: Lexer::new(),
            sections: Vec::new(),
            total_words: 0,
            heading_width: 7,
        }
    }

    pub fn push_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map(BufReader::new)?;
        self.push_stream(file)
    }

    pub fn push_stream(&mut self, stream: impl BufRead) -> Result<()> {
        fn format_heading(heading: &str) -> String {
            heading.trim_left_matches(|c| c == '#').trim().into()
        }

        let mut lexemes = self.lexer.lexemes(stream);
        let mut section = None;
        let mut stats = Stats::default();
        let mut paragraph = 0;

        while let Some(lexeme) = lexemes.next() {
            match lexeme {
                Err(e) => return Err(e),

                Ok(Lexeme::Heading(heading)) => {
                    self.heading_width = cmp::max(heading.len(), self.heading_width);

                    match section.take() {
                        // We've just found a heading and we didn't already have one. Any text
                        // appearing before now should not be stored under this heading name, but
                        // rather under an unknown heading name. This case is probably rare.
                        None => {
                            section = Some(format_heading(heading));
                            if !stats.is_empty() {
                                self.sections.push((None, stats));
                                stats = Stats::default();
                            }
                        }

                        // The normal header case; we push the current heading name and
                        // accumulated stats.
                        Some(completed_section) => {
                            self.sections.push((Some(completed_section), stats));
                            section = Some(format_heading(heading));
                            stats = Stats::default();
                        }
                    }
                }

                // Text should be accumulated via the paragraph.
                Ok(Lexeme::Text(text)) => {
                    paragraph += self.lexer.words(text).count() as u32;
                }

                // Whitespace signals a complete paragraph.
                Ok(Lexeme::Whitespace(_)) if paragraph != 0 => {
                    stats.push_paragraph(paragraph);
                    paragraph = 0;
                }

                // Eat whitespace where no words have been accumulated.
                Ok(Lexeme::Whitespace(_)) => {}

                // Eat comments.
                Ok(Lexeme::Comment(_)) => {}
            }
        }

        if paragraph != 0 {
            stats.push_paragraph(paragraph);
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
                width = derive_width(self.heading_width)
            )?;
        }

        writeln!(f, "\nTotal words: {}", self.total_words)
    }
}

#[cfg(test)]
mod tests {
    use stats::Collector;
    use std::io::Cursor;

    static TEXT: &str = include_str!("../resources/stats.txt");

    #[test]
    fn stats_are_correct() {
        let stream = Cursor::new(TEXT);
        let mut collector = Collector::new();

        collector.push_stream(stream).unwrap();

        // Check word count
        assert_eq!(321, collector.total_words);

        // Check paragraph count
        let (_, stats) = collector.sections.into_iter().next().unwrap();
        assert_eq!(9, stats.paragraphs);
    }
}
