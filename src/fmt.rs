use std::io;

use crate::document::RootDocument;

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

    pub fn format(&self, document: &RootDocument) -> io::Result<()> {
        todo!()
    }
}
