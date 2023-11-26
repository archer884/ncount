use std::io;

use crate::document::Document;

#[derive(Debug, Default)]
pub struct StatFmt {
    filter: Option<String>,
}

impl StatFmt {
    pub fn with_filter(filter: impl Into<String>) -> Self {
        Self {
            filter: Some(filter.into()),
        }
    }

    pub fn format(&self, document: &Document) -> io::Result<()> {
        let document = self.apply_filter(document).unwrap_or(document);
        format_document(document)
    }

    fn apply_filter<'a>(&self, document: &'a Document) -> Option<&'a Document> {
        self.filter
            .as_ref()
            .and_then(|heading| document.get_heading(&heading.to_ascii_uppercase()))
    }
}

fn format_document(document: &Document) -> io::Result<()> {
    println!("{}", document.total_count());
    println!("{document:#?}");
    Ok(())
}
