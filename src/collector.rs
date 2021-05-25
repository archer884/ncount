use std::{collections::VecDeque, fs, io, path::Path};

mod heading;
mod stats;

use prettytable::{
    format::{Alignment, TableFormat},
    Cell, Table,
};

use heading::Heading;
use regex::bytes::{Regex, RegexBuilder};
use stats::Stats;

#[derive(Debug)]
pub struct DocumentStats {
    level: u8,
    heading: String,
    stats: Stats,
    children: Vec<DocumentStats>,
}

impl DocumentStats {
    pub fn new() -> Self {
        Self {
            level: 0,
            heading: String::new(),
            stats: Stats::default(),
            children: Vec::new(),
        }
    }

    fn with_level(level: u8) -> Self {
        Self {
            level,
            ..Self::new()
        }
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
            if line.is_empty() || !line.bytes().any(|u| u.is_ascii_alphanumeric()) {
                continue;
            }

            // Lines beginning with # are markdown headings
            // The other kind of heading is not permitted. Get over it.
            if line.starts_with('#') {
                match heading.take() {
                    Some(last_heading) => {
                        self.append_stats(last_heading, stats);
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
            level: self.level + 1,
            text: filename.to_string(),
        });
        self.last_child().append_stats(heading, stats);
    }

    pub fn filter_by_heading(&mut self, filter: &str) {
        let filter = HeadingFilter::new(filter);
        self.filter_self(&filter);
    }

    fn filter_self(&mut self, filter: &HeadingFilter) -> bool {
        if filter.is_match(&self.heading) {
            true
        } else {
            // Gather indices of any matching children.
            let mut indices: VecDeque<_> = self
                .children
                .iter_mut()
                .enumerate()
                .filter_map(|(idx, child)| child.filter_self(filter).then(|| idx))
                .collect();

            // Retain only matching children.
            let mut idx = 0;
            self.children.retain(|_child| {
                let is_retained_idx = indices.front().map(|&fidx| fidx == idx).unwrap_or_default();
                idx += 1;

                if is_retained_idx {
                    indices.pop_front();
                }
                is_retained_idx
            });

            // Because this level is retained only as a parent heading for
            // a valid subheading, text at this level is no longer counted
            // for statistical purposes.
            self.stats = Stats::default();
            !self.children.is_empty()
        }
    }

    pub fn overall_stats(&self) -> Stats {
        self.stats
            + self
                .children
                .iter()
                .map(|child| child.overall_stats())
                .collect()
    }

    pub fn as_table(&self, detail: bool) -> Table {
        let mut table = Table::new();

        add_format(&mut table);
        add_header(&mut table, detail);
        self.visit_rows(&mut VisitRowsContext {
            table: &mut table,
            count: 0,
            detail,
        });

        // Nothing the footer prints makes sense if we're only printing the word count.
        if detail {
            add_footer(&mut table, self.overall_stats());
        }

        table
    }

    fn append_stats(&mut self, heading: Heading, stats: Stats) {
        if heading.level > self.level + 1 {
            self.last_child().append_stats(heading, stats);
        } else {
            let mut document = DocumentStats::with_level(heading.level);
            document.heading = heading.text;
            document.stats = stats;
            self.children.push(document);
        }
    }

    fn last_child(&mut self) -> &mut Self {
        if self.children.is_empty() {
            self.children
                .push(DocumentStats::with_level(self.level + 1));
        }

        self.children.last_mut().unwrap()
    }

    fn visit_rows(&self, context: &mut VisitRowsContext) {
        add_row(context, &self.heading, self.level, &self.stats);
        self.children
            .iter()
            .for_each(|child| child.visit_rows(context));
    }
}

impl Default for DocumentStats {
    fn default() -> Self {
        Self::new()
    }
}

struct DocumentStatsIter<'a> {
    
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

// FIXME: this interim implementation of add_row mimics the behavior of the old
// add_rows implementation. It has no understanding of nesting, etc.
fn add_row<'a>(context: &'a mut VisitRowsContext, heading: &'a str, _level: u8, stats: &Stats) {
    if heading.is_empty() && stats.is_empty() {
        return;
    }

    let row = context.table.add_empty_row();
    row.add_cell(build_cell(heading, Alignment::LEFT));

    if stats.is_empty() {
        return;
    }

    context.count += stats.word_count;

    if context.detail {
        row.add_cell(build_cell(
            stats.paragraph_count.to_string(),
            Alignment::RIGHT,
        ));
        row.add_cell(build_cell(
            stats.average_paragraph().to_string(),
            Alignment::RIGHT,
        ));
        row.add_cell(build_cell(
            stats.longest_paragraph.to_string(),
            Alignment::RIGHT,
        ));
    }

    row.add_cell(build_cell(stats.word_count.to_string(), Alignment::RIGHT));
    row.add_cell(build_cell(context.count.to_string(), Alignment::RIGHT));
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
}
