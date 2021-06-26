use std::{fs, io, path::Path};

mod heading;
mod stats;

use prettytable::{
    format::{Alignment, TableFormat},
    Cell, Table,
};

use heading::Heading;
use regex::bytes::{Regex, RegexBuilder};
use stats::Stats;

#[derive(Clone, Debug, Default)]
pub struct DocumentStats {
    sections: Vec<Section>,
}

#[derive(Clone, Debug)]
struct Section {
    heading: Heading,
    stats: Stats,
}

impl DocumentStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_path(&mut self, path: &Path) -> crate::Result<()> {
        let filename = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Cannot apply directory path"))?
            .to_string_lossy();

        self.apply_str(&*filename, &fs::read_to_string(path)?);
        Ok(())
    }

    pub fn apply_str(&mut self, filename: &str, text: &str) {
        let text = filter_comments(text);
        let mut heading = None;
        let mut stats = Stats::default();

        for line in text.lines() {
            if !is_content(line) {
                continue;
            }

            // Lines beginning with # are markdown headings
            // The other kind of heading is not permitted. Get over it.
            if line.starts_with('#') {
                match heading.take() {
                    Some(last_heading) => {
                        self.sections.push(Section {
                            heading: last_heading,
                            stats,
                        });
                        heading = Some(Heading::from_str(line));
                        stats = Stats::default();
                    }
                    None => heading = Some(Heading::from_str(line)),
                }
            } else {
                stats.push(word_count(line));
            }
        }

        let heading = heading.take().unwrap_or_else(|| Heading {
            level: 0,
            text: filename.to_string(),
        });

        self.sections.push(Section { heading, stats });
    }

    pub fn filter_by_heading(&mut self, filter: &str) {
        // We need to know that a given heading either
        // A) matches the filter provided or
        // B) is a descendant of a matching heading.

        let filter = HeadingFilter::new(filter);
        let mut level = None;

        self.sections.retain(|section| {
            // If this section is a descendant of a retained heading, retain
            // it also. If not, we will no longer consider subsequent
            // headings to be descendants.
            let is_descendant_heading = level
                .map(|level| section.heading.level > level)
                .unwrap_or_default();
            if is_descendant_heading {
                return true;
            } else {
                level = None;
            }

            if filter.is_match(&section.heading.text) {
                level = Some(section.heading.level);
                true
            } else {
                false
            }
        })
    }

    pub fn overall_stats(&self) -> Stats {
        self.sections.iter().map(|section| &section.stats).collect()
    }

    pub fn as_table(&self, detail: bool) -> Table {
        let mut table = Table::new();

        add_format(&mut table);
        add_header(&mut table, detail);
        add_rows(
            &self.sections,
            &mut VisitRowsContext {
                table: &mut table,
                count: 0,
                detail,
            },
        );

        // Nothing the footer prints makes sense if we're only printing the word count.
        if detail {
            add_footer(&mut table, self.overall_stats());
        }

        table
    }
}

fn is_content(text: &str) -> bool {
    text.bytes().any(|u| u.is_ascii_alphanumeric())
        && !text
            .get(0..2)
            .map(|slice| slice == "[^")
            .unwrap_or_default()
}

struct VisitRowsContext<'a> {
    table: &'a mut Table,
    count: u32,
    detail: bool,
}

enum HeadingFilter {
    Regex(Regex),
    Text(String),
}

impl HeadingFilter {
    fn new(filter: &str) -> Self {
        RegexBuilder::new(filter)
            .case_insensitive(true)
            .build()
            .map(HeadingFilter::Regex)
            .unwrap_or_else(|_| HeadingFilter::Text(filter.to_ascii_lowercase()))
    }

    fn is_match(&self, heading: &str) -> bool {
        match self {
            HeadingFilter::Regex(regex) => regex.is_match(heading.as_bytes()),
            HeadingFilter::Text(text) => heading.to_ascii_lowercase().contains(text),
        }
    }
}

fn word_count(text: &str) -> u32 {
    // Words are usually separated by spaces, but they
    // could be separated by m-dashes instead. We do not
    // count hyphenated words as two words.
    //
    // The filter has been added in order to prevent
    // quotes, followed by emdashes, being counted as
    // words.
    text.split_whitespace()
        .flat_map(|s| s.split("---"))
        .filter(|&s| s.bytes().any(|u| u.is_ascii_alphanumeric()))
        .count() as u32
}

fn filter_comments(text: &str) -> String {
    let mut text = text;
    let mut state = false;
    let mut result = String::with_capacity(text.len());

    while !text.is_empty() {
        if !state {
            if let Some(idx) = text.find("<!--") {
                state = true;
                result.push_str(&text[..idx]);
                text = &text[idx..];
            } else {
                result.push_str(text);
                return result;
            }
        } else if let Some(idx) = text.find("-->") {
            state = false;
            text = &text[(idx + 3)..];
        }
    }

    result
}

fn add_format(table: &mut Table) {
    let mut format = TableFormat::new();
    format.borders(' ');
    format.padding(0, 3);
    table.set_format(format);
}

fn add_header(table: &mut Table, detail: bool) {
    let row = table.add_empty_row();
    row.add_cell(build_cell("§", Alignment::LEFT));

    if detail {
        row.add_cell(build_cell("Count ¶", Alignment::RIGHT));
        row.add_cell(build_cell("Avg ¶", Alignment::RIGHT));
        row.add_cell(build_cell("Long ¶", Alignment::RIGHT));
    }

    row.add_cell(build_cell("Words", Alignment::RIGHT));
    row.add_cell(build_cell("Total", Alignment::RIGHT));
}

fn add_rows(sections: &[Section], context: &mut VisitRowsContext) {
    for section in sections {
        let row = context.table.add_empty_row();
        row.add_cell(build_cell(&section.heading.text, Alignment::LEFT));

        if section.stats.is_empty() {
            continue;
        }

        context.count += section.stats.word_count;

        if context.detail {
            row.add_cell(build_cell(
                section.stats.paragraph_count.to_string(),
                Alignment::RIGHT,
            ));
            row.add_cell(build_cell(
                section.stats.average_paragraph().to_string(),
                Alignment::RIGHT,
            ));
            row.add_cell(build_cell(
                section.stats.longest_paragraph.to_string(),
                Alignment::RIGHT,
            ));
        }

        row.add_cell(build_cell(
            section.stats.word_count.to_string(),
            Alignment::RIGHT,
        ));
        row.add_cell(build_cell(context.count.to_string(), Alignment::RIGHT));
    }
}

fn add_footer(table: &mut Table, stats: Stats) {
    let row = table.add_empty_row();
    row.add_cell(Cell::new_align("", Alignment::LEFT));
    row.add_cell(Cell::new_align(
        &stats.paragraph_count.to_string(),
        Alignment::RIGHT,
    ));
    row.add_cell(Cell::new_align(
        &stats.average_paragraph().to_string(),
        Alignment::RIGHT,
    ));
    row.add_cell(Cell::new_align(
        &stats.longest_paragraph.to_string(),
        Alignment::RIGHT,
    ));
    row.add_cell(Cell::new_align(
        &stats.word_count.to_string(),
        Alignment::RIGHT,
    ));
}

fn build_cell(content: impl AsRef<str>, alignment: Alignment) -> Cell {
    Cell::new_align(content.as_ref(), alignment)
}

#[cfg(test)]
mod tests {
    use super::{DocumentStats, Stats};

    static TEXT: &str = include_str!("../resources/sample.md");

    #[test]
    fn stats_are_correct() {
        let mut document = DocumentStats::new();
        document.apply_str("Foo", TEXT);

        let Stats {
            paragraph_count,
            word_count,
            ..
        } = document.overall_stats();

        assert_eq!(321, word_count, "{:?}", document.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", document.overall_stats());
    }

    #[test]
    fn count_handles_quotes_and_dashes() {
        let text = "---what?!";
        let mut document = DocumentStats::new();

        document.apply_str("Foo", text);

        let Stats {
            paragraph_count,
            word_count,
            ..
        } = document.overall_stats();

        assert_eq!(1, paragraph_count);
        assert_eq!(1, word_count);
    }

    #[test]
    fn count_is_filtered_correctly() {
        let mut document = DocumentStats::new();
        document.apply_str("Foo", TEXT);
        document.filter_by_heading("Other title");
        let Stats {
            word_count,
            paragraph_count,
            longest_paragraph,
        } = document.overall_stats();

        assert_eq!(82, word_count);
        assert_eq!(4, paragraph_count);
        assert_eq!(41, longest_paragraph);
    }

    #[test]
    fn footnotes_are_not_counted() {
        // My mind clearly has a very negative bias for some reason.
        let text = "Hello, world!\n\n\
            How are you?[^note]\n\n\
            [^note]: No one cares.";
        
        let mut document = DocumentStats::new();
        document.apply_str("foo.txt", text);
        let Stats { word_count, .. } = document.overall_stats();

        assert_eq!(5, word_count);
    }
}
