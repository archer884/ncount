mod stats;

use prettytable::{
    format::{Alignment, TableFormat},
    Cell, Table,
};
use stats::Stats;

#[derive(Debug, Default)]
pub struct Collector {
    stats: Vec<TaggedStats>,
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
        Collector { stats: Vec::new() }
    }

    pub fn apply_str(&mut self, filename: &str, text: &str) {
        let text = filter_comments(text);
        let mut heading = None;
        let mut stats = Stats::default();

        for line in text.lines() {
            if line.is_empty() || !line.bytes().any(|u| u.is_ascii_alphabetic()) {
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

fn heading_name(s: &str) -> String {
    s.trim_start_matches(|x: char| x == '#' || x.is_whitespace())
        .to_owned()
}

fn filter_comments(text: &str) -> String {
    let mut text = text;
    let mut state = false;
    let mut result = String::new();

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

#[cfg(test)]
mod tests {
    use super::{Collector, Stats};

    static TEXT: &str = include_str!("../resources/sample.md");

    #[test]
    fn stats_are_correct() {
        let mut collector = Collector::default();
        collector.apply_str("Foo", TEXT);

        let Stats {
            word_count,
            paragraph_count,
            ..
        } = collector.overall_stats();

        assert_eq!(321, word_count, "{:?}", collector.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", collector.overall_stats());
    }
}

fn build_cell(content: impl AsRef<str>, alignment: Alignment) -> Cell {
    Cell::new_align(content.as_ref(), alignment)
}
