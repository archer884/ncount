use std::fmt::{self, Display};

use ncount::DocumentStats;

pub struct ChapterFormatter<'a> {
    document: &'a DocumentStats,
}

impl<'a> ChapterFormatter<'a> {
    pub fn new(document: &'a DocumentStats) -> Self {
        ChapterFormatter { document }
    }
}

impl<'a> Display for ChapterFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
