use std::{fs, io, path::Path};

mod stats;

use prettytable::{
    format::{Alignment, TableFormat},
    Cell, Table,
};

use regex::{Regex, bytes::RegexBuilder};
use stats::Stats;

#[derive(Debug)]
pub struct Collector {
    stats: Vec<TaggedStats>,
    comment_pattern: Regex,
}

#[derive(Debug)]
pub struct TaggedStats {
    tag: String,
    stats: Stats,
}

impl TaggedStats {
    fn new(tag: impl Into<String>, stats: Stats) -> Self {
        Self {
            tag: tag.into(),
            stats,
        }
    }

    fn tag(&self) -> &str {
        &self.tag
    }
}

impl Collector {
    pub fn new() -> Collector {
        Collector {
            stats: Vec::new(),
            comment_pattern: Regex::new(r#"(?s)<!--.*?-->"#).unwrap(),
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
        let text = self.comment_pattern.replace_all(text, "");
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
                    None => heading = Some(heading_name(line)),
                    Some(last_heading) => {
                        self.push(last_heading, stats);
                        heading = Some(heading_name(line));
                        stats = Stats::default();
                    }
                }
            } else {
                stats.push(self.word_count(line));
            }
        }

        self.push(
            heading.as_ref().map(AsRef::as_ref).unwrap_or(filename),
            stats,
        );
    }

    fn word_count(&self, s: &str) -> u32 {
        // Words are usually separated by spaces, but they
        // could be separated by m-dashes instead. We do not
        // count hyphenated words as two words.
        s.split_whitespace().flat_map(|s| s.split("---")).count() as u32
    }

    pub fn push(&mut self, heading: impl Into<String>, stats: Stats) {
        self.stats.push(TaggedStats::new(heading, stats));
    }

    pub fn filter_by_heading(&mut self, filter: &str) {
        match RegexBuilder::new(filter).case_insensitive(true).build() {
            Ok(filter) => {
                self.stats.retain(|x| filter.is_match(x.tag().as_bytes()));
            }
            Err(_) => {
                let filter = filter.to_ascii_lowercase();
                self.stats
                    .retain(|x| x.tag().to_ascii_lowercase().contains(&filter))        
            }
        }
    }

    pub fn overall_stats(&self) -> Stats {
        self.stats.iter().map(|x| &x.stats).collect()
    }

    pub fn as_table(&self, detail: bool) -> Table {
        let mut table = Table::new();

        add_format(&mut table);
        add_header(&mut table, detail);
        add_rows(&mut table, &self.stats, detail);

        // Nothing the footer prints makes sense if we're only printing the word count.
        if detail {
            add_footer(&mut table, self.overall_stats());
        }

        table
    }
}

/// Extract the heading name from a heading line
///
/// This function is also meant to remove formatting so that the heading can
/// be displayed without things like asterisks, etc.
fn heading_name(s: &str) -> String {
    s.trim_start_matches(|u: char| u == '#' || u.is_whitespace())
        .replace(|u: char| u == '*' || u == '_', "")
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

fn add_rows<'a>(
    table: &mut Table,
    data: impl IntoIterator<Item = &'a TaggedStats> + 'a,
    detail: bool,
) {
    let mut running_count = 0;
    for item in data {
        let stats = &item.stats;
        running_count += stats.word_count;

        let row = table.add_empty_row();
        row.add_cell(build_cell(item.tag(), Alignment::LEFT));

        if stats.is_empty() {
            continue;
        }

        if detail {
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
        row.add_cell(build_cell(running_count.to_string(), Alignment::RIGHT));
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
    use super::{Collector, Stats};

    static TEXT: &str = include_str!("../resources/sample.md");

    #[test]
    fn stats_are_correct() {
        let mut collector = Collector::new();
        collector.apply_str("Foo", TEXT);

        let Stats {
            paragraph_count,
            word_count,
            ..
        } = collector.overall_stats();

        assert_eq!(321, word_count, "{:?}", collector.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", collector.overall_stats());
    }
}
