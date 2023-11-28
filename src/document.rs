use std::{iter, ops};

use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct DocumentBuilder {
    root: Document,
    current_level: i32,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            root: Document::new(0),
            current_level: 0,
        }
    }

    pub fn finalize(self) -> Document {
        self.root
    }

    pub fn apply(&mut self, s: impl AsRef<str>) {
        // It is the responsibility of the document builder to count the number of words in a
        // string. However, it is my intention to provide the document builder only strings which
        // have been cleaned with regard to comments and notes.

        let paragraphs = s.as_ref().lines().filter_map(|line| {
            let line = line.trim();
            if !line.is_empty() {
                Some(line)
            } else {
                None
            }
        });

        // At this point, we are concerned with two types of lines (paragraph-level elements):
        // - Paragraphs containing text
        // - Headings containing titles and characterized by some level or other
        // A paragraph is applied to the "current" document. However, a heading triggers the
        // generation of a new document instead. The question of how we keep track of which
        // document is the "current" document is... left as an exercise to the reader.

        fn try_get_heading(s: &str) -> Option<(&str, i32)> {
            if !s.starts_with('#') {
                return None;
            }

            let level = s.bytes().take_while(|&u| u == b'#').count() as i32;
            let heading = s.trim_start_matches('#').trim();
            Some((heading, level))
        }

        let mut target = self.root.current_document(self.current_level);
        for s in paragraphs {
            // If this line turns out to be a heading, we need to update our current level and
            // update our target document. Otherwise, we're just going to continue with our
            // current target.

            if let Some((heading, level)) = try_get_heading(s) {
                tracing::debug!(
                    heading,
                    level,
                    current_level = self.current_level,
                    "requesting current document"
                );
                target = self.root.new_document(level);
                target.set_heading(heading);
                self.current_level = level;
                continue;
            }

            // Now that we have a target, we just need to apply the actual text.
            target.add_paragraph(s.unicode_words().count() as u32);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    heading: Option<String>,
    level: i32,
    paragraphs: Paragraphs,
    subdocuments: Vec<Document>,
}

impl Document {
    fn new(level: i32) -> Self {
        Self {
            heading: None,
            level,
            paragraphs: Paragraphs::new(),
            subdocuments: Vec::new(),
        }
    }

    pub fn get_heading(&self, heading: &str) -> Option<&Document> {
        let document = self.subdocuments.iter().find(|&x| {
            x.heading
                .as_ref()
                .map(|x| x.to_ascii_uppercase().starts_with(heading))
                .unwrap_or_default()
        });

        let mut fallback = self
            .subdocuments
            .iter()
            .filter_map(|x| x.get_heading(heading));
        document.or_else(|| fallback.next())
    }
}

impl Document {
    fn current_document(&mut self, level: i32) -> &mut Document {
        let delta = level - self.level;
        debug_assert!(delta >= 0, "impossible level requested");
        match delta {
            0 => self,
            1 => self.last_document(),
            _ => self.last_document().current_document(level),
        }
    }

    fn last_document(&mut self) -> &mut Document {
        if self.subdocuments.is_empty() {
            self.subdocuments.push(Document::new(self.level + 1));
        }
        self.subdocuments.last_mut().unwrap()
    }

    fn new_document(&mut self, level: i32) -> &mut Document {
        let delta = level - self.level;
        debug_assert!(delta > 0, "impossible level requested");
        match delta {
            // Can this ever happen? ...Not with that debugassert in place, but...
            0 => self,
            1 => {
                self.subdocuments.push(Document::new(level));
                self.subdocuments.last_mut().unwrap()
            }
            _ => self.last_document().new_document(level),
        }
    }

    fn add_paragraph(&mut self, p: u32) {
        self.paragraphs.add(p);
    }

    fn set_heading(&mut self, heading: &str) {
        self.heading = Some(heading.into());
    }

    pub fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = DocumentStats> + 'a> {
        let subdocs = self.subdocuments.iter().flat_map(|x| x.iter());
        if self.heading.is_some() {
            Box::new(iter::once(DocumentStats(self)).chain(subdocs))
        } else {
            Box::new(subdocs)
        }
    }
}

pub struct DocumentStats<'a>(&'a Document);

impl DocumentStats<'_> {
    pub fn heading(&self) -> Option<&str> {
        self.0.heading.as_deref()
    }

    pub fn level(&self) -> i32 {
        self.0.level
    }

    pub fn paragraphs(&self) -> Paragraphs {
        self.0.paragraphs
    }
}

/// A summary of the paragraphs of a document section
#[derive(Clone, Copy, Debug, Default)]
pub struct Paragraphs {
    /// count of the paragraphs in the section
    pub count: u32,
    /// length of the longest paragraph
    pub max: u32,
    /// length of the shortest paragraph
    pub min: u32,
    /// total length of all paragraphs
    pub total: u32,
}

impl Paragraphs {
    fn new() -> Self {
        Default::default()
    }

    fn add(&mut self, p: u32) {
        self.count += 1;
        self.max = self.max.max(p);
        self.min = self.min.min(p);
        self.total += p;
    }

    pub fn is_zero(&self) -> bool {
        self.count == 0
    }

    pub fn average_len(&self) -> u32 {
        (self.total as f64 / self.count as f64).round() as u32
    }
}

#[derive(Debug, Default)]
pub struct OverallStats {
    /// count of all paragraphs
    pub count: u32,
    /// length of the longest paragraph
    pub max: u32,
    /// length of the shortest paragraph
    pub min: u32,
    /// total length of all paragraphs
    pub total: u32,
}

impl OverallStats {
    pub fn average_len(&self) -> u32 {
        (self.total as f64 / self.count as f64).round() as u32
    }
}

impl<'a> ops::AddAssign<DocumentStats<'a>> for OverallStats {
    fn add_assign(&mut self, rhs: DocumentStats<'a>) {
        let p = rhs.paragraphs();
        self.count += p.count;
        self.max = self.max.max(p.max);
        self.min = self.min.min(p.min);
        self.total += p.total;
    }
}

impl<'a> FromIterator<DocumentStats<'a>> for OverallStats {
    fn from_iter<T: IntoIterator<Item = DocumentStats<'a>>>(iter: T) -> Self {
        let mut stats = OverallStats::default();
        for p in iter {
            stats += p;
        }
        stats
    }
}
