use std::{
    borrow::Cow,
    io::{self, Write},
};

use prettytable::{
    format::{Alignment, TableFormat},
    Cell, Table,
};

use crate::document::{Document, DocumentStats, OverallStats};

#[derive(Debug, Default)]
pub struct StatFmt {
    filter: Option<String>,
    verbose: bool,
    running_count: u32,
}

impl StatFmt {
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            filter: None,
            running_count: 0,
        }
    }

    pub fn add_filter(&mut self, filter: impl Into<String>) {
        self.filter = Some(filter.into());
    }

    pub fn format(&mut self, document: &Document) -> io::Result<()> {
        let document = self.apply_filter(document).unwrap_or(document);
        self.format_document(document)
    }

    fn format_document(&mut self, document: &Document) -> io::Result<()> {
        let mut table = self.build_formatted_table();
        for stats in document.iter() {
            self.add_row(&mut table, stats);
        }

        if self.verbose {
            let sum: OverallStats = document.iter().collect();
            let row = table.add_empty_row();
            row.add_cell(Cell::new_align("", Alignment::LEFT));
            row.add_cell(Cell::new_align(&sum.count.to_string(), Alignment::RIGHT));
            row.add_cell(Cell::new_align(
                &sum.average_len().to_string(),
                Alignment::RIGHT,
            ));
            row.add_cell(Cell::new_align(&sum.max.to_string(), Alignment::RIGHT));
            row.add_cell(Cell::new_align(&sum.total.to_string(), Alignment::RIGHT));
        }

        writeln!(io::stdout().lock(), "{table}")
    }

    fn add_row(&mut self, table: &mut Table, stats: DocumentStats) {
        let row = table.add_empty_row();

        if let Some(heading) = stats.heading() {
            let heading = match stats.level() {
                0 | 1 => Cow::from(heading),
                2 => Cow::from(format!(" {heading}")),
                3 => Cow::from(format!("  {heading}")),
                4 => Cow::from(format!("   {heading}")),
                _ => Cow::from(format!("    {heading}")),
            };
            row.add_cell(Cell::new_align(&heading, Alignment::LEFT).style_spec("b"));
        } else {
            return;
        }

        if stats.paragraphs().is_zero() {
            return;
        }

        self.running_count += stats.paragraphs().total;

        if self.verbose {
            row.add_cell(Cell::new_align(
                &stats.paragraphs().count.to_string(),
                Alignment::RIGHT,
            ));
            row.add_cell(Cell::new_align(
                &stats.paragraphs().average_len().to_string(),
                Alignment::RIGHT,
            ));
            row.add_cell(Cell::new_align(
                &stats.paragraphs().max.to_string(),
                Alignment::RIGHT,
            ));
        }

        row.add_cell(Cell::new_align(
            &stats.paragraphs().total.to_string(),
            Alignment::RIGHT,
        ));
        row.add_cell(Cell::new_align(
            &self.running_count.to_string(),
            Alignment::RIGHT,
        ));
    }

    fn apply_filter<'a>(&self, document: &'a Document) -> Option<&'a Document> {
        self.filter
            .as_ref()
            .and_then(|heading| document.get_heading(&heading.to_ascii_uppercase()))
    }

    /// Builds a table with appropriate format and headers.
    fn build_formatted_table(&self) -> Table {
        let mut format = TableFormat::new();
        format.borders(' ');
        format.padding(0, 3);

        let mut table = Table::new();
        table.set_format(format);

        let row = table.add_empty_row();
        row.add_cell(Cell::new_align("§", Alignment::LEFT));
        if self.verbose {
            row.add_cell(Cell::new_align("Count ¶", Alignment::RIGHT));
            row.add_cell(Cell::new_align("Avg ¶", Alignment::RIGHT));
            row.add_cell(Cell::new_align("Long ¶", Alignment::RIGHT));
        }
        row.add_cell(Cell::new_align("Words", Alignment::RIGHT));
        row.add_cell(Cell::new_align("Total", Alignment::RIGHT));

        table
    }
}
