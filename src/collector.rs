use prettytable::Table;
use std::cmp;

#[derive(Debug)]
pub struct Collector {
    stats: Vec<(Option<String>, Stats)>,
}

#[derive(Debug, Default)]
pub struct Stats {
    pub word_count: u32,
    pub paragraph_count: u32,
    pub longest_paragraph: u32,
}

impl Stats {
    pub fn push(&mut self, count: u32) {
        self.word_count += count;
        self.paragraph_count += 1;
        self.longest_paragraph = cmp::max(self.longest_paragraph, count);
    }

    pub fn average_paragraph(&self) -> u32 {
        match self.paragraph_count {
            0 => 0,
            x => self.word_count / x,
        }
    }
}

impl Collector {
    pub fn new() -> Collector {
        Collector { stats: Vec::new() }
    }

    pub fn as_table(&self) -> Table {
        use prettytable::{
            format::{Alignment, TableFormat},
            Cell,
        };

        let mut table = Table::new();
        let mut format = TableFormat::new();
        format.borders(' ');
        format.padding(0, 3);
        table.set_format(format);

        {
            let row = table.add_empty_row();
            row.add_cell(Cell::new_align("§", Alignment::LEFT));
            row.add_cell(Cell::new_align("Count ¶", Alignment::RIGHT));
            row.add_cell(Cell::new_align("Avg ¶", Alignment::RIGHT));
            row.add_cell(Cell::new_align("Long ¶", Alignment::RIGHT));
            row.add_cell(Cell::new_align("Words", Alignment::RIGHT));
        }

        for (heading, stats) in self.stats.iter() {
            let row = table.add_empty_row();
            row.add_cell(Cell::new_align(
                heading.as_ref().map(AsRef::as_ref).unwrap_or("Untitled"),
                Alignment::LEFT,
            ));
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

        {
            let row = table.add_empty_row();
            let stats = self.overall_stats();
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

        table
    }

    pub fn apply_str(&mut self, filename: &str, text: &str) -> crate::Result<()> {
        let text = filter_comments(text);
        let mut heading = None;
        let mut stats = Stats::default();

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }

            // Lines beginning with # are markdown headings
            // The other kind of heading is not permitted. Get over it.
            if line.starts_with('#') {
                match heading.take() {
                    None => heading = Some(heading_name(line)),
                    Some(last_heading) => {
                        self.push(last_heading, stats);
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

        Ok(())
    }

    fn word_count(&self, s: &str) -> u32 {
        // Words are usually separated by spaces, but they
        // could be separated by m-dashes instead. We do not
        // count hyphenated words as two words.
        s.split_whitespace().flat_map(|s| s.split("---")).count() as u32
    }

    pub fn push(&mut self, heading: impl Into<String>, stats: Stats) {
        self.stats.push((Some(heading.into()), stats));
    }

    pub fn overall_stats(&self) -> Stats {
        let (word_count, paragraph_count, longest_paragraph) = self.stats.iter().fold(
            (0, 0, 0),
            |(word_count, paragraph_count, longest_paragraph), stats| {
                (
                    word_count + stats.1.word_count,
                    paragraph_count + stats.1.paragraph_count,
                    cmp::max(longest_paragraph, stats.1.longest_paragraph),
                )
            },
        );

        Stats {
            word_count,
            paragraph_count,
            longest_paragraph,
        }
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

#[cfg(test)]
mod tests {
    use super::{Collector, Stats};

    static TEXT: &str = include_str!("../resources/sample.md");

    #[test]
    fn stats_are_correct() {
        let mut collector = Collector::new();
        collector.apply_str("Foo", TEXT).unwrap();

        let Stats {
            word_count,
            paragraph_count,
            ..
        } = collector.overall_stats();

        assert_eq!(321, word_count, "{:?}", collector.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", collector.overall_stats());
    }
}
