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
            target.add_paragraph(s.unicode_words().count() as i32);
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

    // FIXME: probably just delete this when you're done with it, right?
    pub fn total_count(&self) -> i32 {
        self.paragraphs.total
            + self
                .subdocuments
                .iter()
                .map(|x| x.total_count())
                .sum::<i32>()
    }

    pub fn get_heading(&self, heading: &str) -> Option<&Document> {
        let uheading = unicase::UniCase::new(heading);
        let document = self.subdocuments.iter().find(|&x| {
            x.heading
                .as_ref()
                .map(|x| unicase::UniCase::new(x) == uheading)
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
            _ => self.last_document().new_document(level)
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
