use std::{iter, ops};

use compact_str::CompactString;
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
            target.add_paragraph(count_words(s));
        }
    }
}

/// Word count matching `unicode_words()` semantics, with a byte-scanning
/// fast path for the (overwhelmingly common, in prose) pure-ASCII case.
/// Falls back to full Unicode segmentation whenever a line isn't ASCII, so
/// correctness for non-ASCII text is inherited directly from the crate.
fn count_words(s: &str) -> u32 {
    if s.is_ascii() {
        ascii_word_count(s)
    } else {
        s.unicode_words().count() as u32
    }
}

/// ASCII word count matched to `unicode_words()` by empirical probing: only
/// `' . : , ;` ever join two word runs, and only a single occurrence
/// between the right adjacent character classes (letters for `'` `.` `:`,
/// digits for `'` `.` `,` `;`) — e.g. "don't" and "3.14" stay one word each,
/// but "co-authored" splits in two since hyphens never join. See the
/// `ascii_word_count_matches_unicode_words` test for the full case list this
/// was validated against. Caller must guarantee `s.is_ascii()`.
fn ascii_word_count(s: &str) -> u32 {
    let b = s.as_bytes();
    let n = b.len();

    fn is_word(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_'
    }
    fn is_alpha(c: u8) -> bool {
        c.is_ascii_alphabetic()
    }
    fn is_digit(c: u8) -> bool {
        c.is_ascii_digit()
    }
    fn joins(prev: u8, c: u8, next: u8) -> bool {
        match c {
            b'\'' | b'.' => {
                (is_alpha(prev) && is_alpha(next)) || (is_digit(prev) && is_digit(next))
            }
            b':' => is_alpha(prev) && is_alpha(next),
            b',' | b';' => is_digit(prev) && is_digit(next),
            _ => false,
        }
    }

    let mut count = 0u32;
    let mut i = 0;
    while i < n {
        if !is_word(b[i]) {
            i += 1;
            continue;
        }
        let mut has_alnum = b[i] != b'_';
        i += 1;
        loop {
            if i < n && is_word(b[i]) {
                has_alnum |= b[i] != b'_';
                i += 1;
                continue;
            }
            if i < n && i + 1 < n && joins(b[i - 1], b[i], b[i + 1]) {
                i += 1;
                continue;
            }
            break;
        }
        if has_alnum {
            count += 1;
        }
    }
    count
}

#[derive(Clone, Debug)]
pub struct Document {
    heading: Option<CompactString>,
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

    pub fn iter(&'_ self) -> Box<dyn Iterator<Item = DocumentStats<'_>> + '_> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn build(text: &str) -> Document {
        let mut builder = DocumentBuilder::new();
        builder.apply(text);
        builder.finalize()
    }

    #[test]
    fn single_heading_single_paragraph_counts_words() {
        let doc = build("# Chapter\n\nSome words here.");
        let stats: Vec<_> = doc.iter().collect();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].heading(), Some("Chapter"));
        assert_eq!(stats[0].level(), 1);
        assert_eq!(stats[0].paragraphs().count, 1);
        assert_eq!(stats[0].paragraphs().total, 3);
    }

    #[test]
    fn multiple_paragraphs_aggregate_count_max_total() {
        let doc = build("# Chapter\n\none two three\n\nfour five\n\nsix");
        let stats: Vec<_> = doc.iter().collect();
        let p = stats[0].paragraphs();
        assert_eq!(p.count, 3);
        assert_eq!(p.max, 3);
        assert_eq!(p.total, 6);
        assert_eq!(p.average_len(), 2);
    }

    #[test]
    fn nested_headings_produce_separate_stats_per_level() {
        let doc = build("# H1\n\ntext one\n\n## H2\n\ntext two three");
        let stats: Vec<_> = doc.iter().collect();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].heading(), Some("H1"));
        assert_eq!(stats[0].level(), 1);
        assert_eq!(stats[0].paragraphs().total, 2);
        assert_eq!(stats[1].heading(), Some("H2"));
        assert_eq!(stats[1].level(), 2);
        assert_eq!(stats[1].paragraphs().total, 3);
    }

    #[test]
    fn heading_with_no_body_is_zero() {
        let doc = build("# Chapter\n\n## Empty\n\n## Next\n\nwords here");
        let stats: Vec<_> = doc.iter().collect();
        let empty = stats.iter().find(|s| s.heading() == Some("Empty")).unwrap();
        assert!(empty.paragraphs().is_zero());
    }

    #[test]
    fn apply_called_multiple_times_accumulates() {
        // Mirrors main.rs's usage: one builder folds over several files in sequence.
        let mut builder = DocumentBuilder::new();
        builder.apply("# Chapter\n\nfirst file words");
        builder.apply("more words in second file");
        let doc = builder.finalize();
        let stats: Vec<_> = doc.iter().collect();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].paragraphs().count, 2);
        assert_eq!(stats[0].paragraphs().total, 3 + 5);
    }

    #[test]
    fn text_before_first_heading_is_not_visible_in_iter() {
        // Characterizes current behavior: paragraphs attached to the headless
        // root document are never yielded by `iter()`, so text preceding the
        // first heading in a file is silently absent from output and totals.
        let doc = build("stray preamble text\n\n# Chapter\n\nreal words");
        let stats: Vec<_> = doc.iter().collect();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].paragraphs().total, 2);
    }

    #[test]
    fn heading_levels_parsed_from_hash_count() {
        let doc = build("# One\n\n### Three\n\nx\n\n##### Five\n\ny");
        let stats: Vec<_> = doc.iter().collect();
        let levels: Vec<_> = stats.iter().map(|s| s.level()).collect();
        assert_eq!(levels, vec![1, 3, 5]);
    }

    #[test]
    fn get_heading_is_case_insensitive_prefix_match() {
        let doc = build("# Chapter One\n\nwords\n\n## Chapter Two\n\nmore words");
        let found = doc.get_heading("CHAPTER TWO").unwrap();
        assert_eq!(found.iter().next().unwrap().heading(), Some("Chapter Two"));

        let prefix = doc.get_heading("CHAPTER O").unwrap();
        assert_eq!(prefix.iter().next().unwrap().heading(), Some("Chapter One"));

        assert!(doc.get_heading("NO SUCH HEADING").is_none());
    }

    #[test]
    fn average_len_rounds_to_nearest() {
        let doc = build("# Chapter\n\na b c\n\nd e");
        // total 5 across 2 paragraphs -> 2.5, rounds to 3 (round-half-away-from-zero)
        assert_eq!(doc.iter().next().unwrap().paragraphs().average_len(), 3);
    }

    #[test]
    fn word_counting_follows_unicode_word_semantics() {
        // Locks in unicode_words()-style joining (contractions stay one word,
        // hyphenated compounds split) regardless of which counter implements it.
        let doc = build("# Chapter\n\nDon't stop, co-authored works, it's 3.14 exactly.");
        let stats: Vec<_> = doc.iter().collect();
        // Don't | stop | co | authored | works | it's | 3.14 | exactly
        assert_eq!(stats[0].paragraphs().total, 8);
    }

    /// Each case is checked against the crate's own `unicode_words()`, so this
    /// stays correct-by-definition even if that implementation ever changes.
    #[test]
    fn ascii_word_count_matches_unicode_words() {
        let cases = [
            "don't",
            "co-authored",
            "U.S.A.",
            "3.14",
            "toughest--situation",
            "the quote's edge",
            "well - actually",
            "it's a 21st-century test",
            "wait...what",
            "ab12.cd34",
            "12ab.34cd",
            "1,234.56",
            "1,234,567",
            "a1'b2",
            "a:1",
            "1:a",
            "a1:b2",
            "won't've",
            "_",
            ".",
            "",
            "a_1.b_2",
            "multi space  gap",
            "ab_.cd",
            "ab_'cd",
            "___",
            "a__a",
            "1__1",
            "rock'n'roll",
            "'tis",
            "foo_bar",
            "1_2",
            "_foo",
            "a..a",
            "a''a",
            "1..1",
            "  leading and trailing spaces  ",
            "multiple...dots.here",
            "a:b:c:d",
            "1,2,3,4",
        ];
        for s in cases {
            assert_eq!(
                ascii_word_count(s),
                s.unicode_words().count() as u32,
                "mismatch on {s:?}"
            );
        }
    }

    #[test]
    fn ascii_word_count_matches_unicode_words_over_sample_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/resource/sample.md");
        let text = std::fs::read_to_string(path).unwrap();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || !line.is_ascii() {
                continue;
            }
            assert_eq!(
                ascii_word_count(line),
                line.unicode_words().count() as u32,
                "mismatch on {line:?}"
            );
        }
    }
}
