use prettytable::Table;
use std::cmp;

#[derive(Debug, Default)]
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
        Default::default()
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

    pub fn push(&mut self, stats: Stats) {
        self.stats.push((None, stats));
    }

    pub fn push_with_heading(&mut self, heading: impl Into<String>, stats: Stats) {
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
