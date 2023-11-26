use unicode_segmentation::UnicodeSegmentation;

trait DocumentStats {
    fn current_document(&mut self, level: i32) -> &mut dyn DocumentStats;
    fn new_document(&mut self, level: i32) -> &mut dyn DocumentStats;
    fn add_paragraph(&mut self, p: i32);
    fn set_heading(&mut self, heading: &str);
}

#[derive(Clone, Debug)]
pub struct DocumentBuilder {
    root: RootDocument,
    current_level: i32,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            root: Default::default(),
            current_level: 0,
        }
    }

    pub fn finalize(self) -> RootDocument {
        self.root
    }

    pub fn apply<S>(&mut self, segments: S)
    where
        S: IntoIterator,
        S::Item: AsRef<str>,
    {
        // It is the responsibility of the document builder to count the number of words in a
        // string. However, it is my intention to provide the document builder only strings which
        // have been cleaned with regard to comments and notes.

        // A given document is made up of segments--which is to say, the whole document with
        // elements like comments and other HTML tags removed. We will replace some tags with text
        // literals, e.g. <br> with \n. Again, however, this is a job for the person providing
        // segments to the document builder--NOT for the document builder itself.

        for segment in segments {
            self.apply_segment(segment.as_ref());
        }
    }

    fn apply_segment(&mut self, s: &str) {
        let paragraphs = s.lines().filter_map(|line| {
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
            target.add_paragraph(s.unicode_words().count() as i32);
        }
    }
}

fn try_get_heading(s: &str) -> Option<(&str, i32)> {
    if !s.starts_with('#') {
        return None;
    }

    let level = s.bytes().take_while(|&u| u == b'#').count() as i32;
    let heading = s.trim_start_matches('#').trim();
    Some((heading, level))
}

#[derive(Clone, Debug, Default)]
pub struct RootDocument {
    // The root document stores paragraphs only in the event
    // that any bare text appears in the corpus.
    paragraphs: Paragraphs,
    subdocuments: Vec<Document>,
}

impl RootDocument {
    fn last_document(&mut self) -> &mut dyn DocumentStats {
        if self.subdocuments.is_empty() {
            self.subdocuments.push(Document::new(1));
        }
        self.subdocuments.last_mut().unwrap()
    }
}

impl DocumentStats for RootDocument {
    fn current_document(&mut self, level: i32) -> &mut dyn DocumentStats {
        match level {
            0 => self,
            1 => self.last_document(),
            _ => self.last_document().current_document(level),
        }
    }

    fn new_document(&mut self, level: i32) -> &mut dyn DocumentStats {
        match level {
            0 => self,
            1 => {
                self.subdocuments.push(Document::new(1));
                self.subdocuments.last_mut().unwrap()
            },
            _ => self.last_document().new_document(level)
        }
    }

    fn add_paragraph(&mut self, p: i32) {
        self.paragraphs.add(p);
    }

    fn set_heading(&mut self, _heading: &str) {
        unreachable!("...please");
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

    fn last_document(&mut self) -> &mut dyn DocumentStats {
        if self.subdocuments.is_empty() {
            self.subdocuments.push(Document::new(self.level + 1));
        }
        self.subdocuments.last_mut().unwrap()
    }
}

impl DocumentStats for Document {
    fn current_document(&mut self, level: i32) -> &mut dyn DocumentStats {
        // We only need to manage the case where the level given is greater than our own level,
        // otherwise we would have already generated a new subdocument at a higher level of
        // recursion.

        debug_assert_ne!(self.level, level, "impossible level requested");

        // If the level is one greater than our own, we need to generate a new document and return
        // a reference to it. Otherwise, we recurse from the last document in our collection.

        if self.level + 1 == level {
            self.last_document()
        } else {
            self.last_document().current_document(level)
        }
    }

    fn new_document(&mut self, level: i32) -> &mut dyn DocumentStats {
        // Again...

        debug_assert_ne!(self.level, level, "impossible level requested");

        if self.level + 1 == level {
            self.subdocuments.push(Document::new(level));
            self.subdocuments.last_mut().unwrap()
        } else {
            self.last_document().new_document(level)
        }
    }

    fn add_paragraph(&mut self, p: i32) {
        self.paragraphs.add(p);
    }

    fn set_heading(&mut self, heading: &str) {
        self.heading = Some(heading.into());
    }
}

/// A summary of the paragraphs of a document section
#[derive(Clone, Debug, Default)]
struct Paragraphs {
    /// count of the paragraphs in the section
    count: usize,
    /// length of the longest paragraph
    max: i32,
    /// length of the shortest paragraph
    min: i32,
    /// total length of all paragraphs
    total: i32,
}

impl Paragraphs {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add(&mut self, p: i32) {
        self.count += 1;
        self.max = self.max.max(p);
        self.min = self.min.min(p);
        self.total += p;
    }

    pub fn average_len(&self) -> i32 {
        (self.total as f64 / self.count as f64).round() as i32
    }
}
