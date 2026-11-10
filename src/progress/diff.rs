//! Words added and words cut between two versions of the charted text.
//!
//! Net change hides drafting: a scene written in place of its beat sheet
//! nets only the difference. So both sides are counted, from a diff in two
//! passes. Paragraphs are matched first; a run of changed paragraphs is then
//! diffed word by word, so a renamed character costs a word, not a paragraph.
//! A paragraph cut in one place and inserted unchanged in another (a scene
//! reordered, or a file renamed so it sorts elsewhere) was moved, not
//! written, so it counts as neither. Words shared by chance inside a changed run (two unrelated passages both
//! have a "the" and a "she said") don't count as kept: a match shorter than
//! `MIN_KEPT` words with changes on both sides is counted as cut and
//! rewritten.
//!
//! Either way, added − cut always equals the net change.

use std::collections::HashMap;
use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices, capture_diff_slices_by_key};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{DocumentBuilder, count_words};
use crate::filter::{LineEvent, Paragraph, TextFilter};

/// Inside a changed run, the shortest stretch of matching words that
/// counts as kept text rather than a coincidence.
const MIN_KEPT: usize = 3;

/// A paragraph's text, with its pieces joined by `\n`. A line break is
/// always a word boundary, so splitting the joined text into words gives
/// exactly the pieces' words, which is what `words` counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    text: String,
    words: u32,
}

impl Paragraph for Text {
    fn start(piece: &str) -> Self {
        Text {
            text: piece.to_string(),
            words: count_words(piece),
        }
    }

    fn extend(&mut self, piece: &str) {
        self.text.push('\n');
        self.text.push_str(piece);
        self.words += count_words(piece);
    }
}

/// One file as of one day: its paragraphs in order, and where the
/// `--filter` section sits among them.
#[derive(Debug)]
pub struct Version {
    paragraphs: Vec<Text>,
    pub words: u32,
    section: Option<Range<usize>>,
}

impl Version {
    /// `heading` is uppercased, as `Document::get_heading` expects.
    pub fn new(text: &str, lexer: &TextFilter, heading: Option<&str>) -> Self {
        let events: Vec<LineEvent<Text>> = lexer.lex_as(text).collect();

        // The section is whichever heading `get_heading` picks, found by
        // building the same tree the table and the TUI would.
        let found = heading.and_then(|heading| {
            let mut builder = DocumentBuilder::new();
            builder.apply(events.iter().map(|event| match event {
                LineEvent::Heading(text, level) => LineEvent::Heading(text.clone(), *level),
                LineEvent::Paragraph(text) => LineEvent::Paragraph(text.words),
            }));
            builder.finalize().find_heading(heading)
        });

        // (level, index of the first paragraph after it)
        let mut headings = Vec::new();
        let mut paragraphs = Vec::new();
        for event in events {
            match event {
                LineEvent::Heading(_, level) => headings.push((level, paragraphs.len())),
                LineEvent::Paragraph(text) => paragraphs.push(text),
            }
        }

        // A section runs until the next heading at its level or above.
        let section = found.map(|nth| {
            let (level, start) = headings[nth];
            let end = headings[nth + 1..]
                .iter()
                .find(|&&(other, _)| other <= level)
                .map_or(paragraphs.len(), |&(_, end)| end);
            start..end
        });

        Version {
            words: paragraphs.iter().map(|p| p.words).sum(),
            paragraphs,
            section,
        }
    }

    pub fn paragraphs(&self) -> &[Text] {
        &self.paragraphs
    }

    /// The `--filter` section's paragraphs, if this file has it.
    pub fn section(&self) -> Option<&[Text]> {
        self.section.clone().map(|range| &self.paragraphs[range])
    }

    pub fn section_words(&self) -> Option<u32> {
        self.section()
            .map(|section| section.iter().map(|p| p.words).sum())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub added: u32,
    pub cut: u32,
}

impl Change {
    pub fn net(self) -> i64 {
        i64::from(self.added) - i64::from(self.cut)
    }
}

impl std::ops::AddAssign for Change {
    fn add_assign(&mut self, other: Self) {
        self.added += other.added;
        self.cut += other.cut;
    }
}

/// Words added and cut going from `old` to `new`.
pub fn diff<'a>(old: &[&'a Text], new: &[&'a Text]) -> Change {
    let key = |p: &&'a Text| -> &'a str { &p.text };
    let ops: Vec<DiffOp> = capture_diff_slices_by_key(Algorithm::Histogram, old, new, key)
        .into_iter()
        .filter(|op| !matches!(op, DiffOp::Equal { .. }))
        .collect();

    // Pair each inserted paragraph with an identical deleted one, if any.
    let mut deleted: HashMap<&str, Vec<usize>> = HashMap::new();
    for i in ops.iter().flat_map(DiffOp::old_range) {
        deleted.entry(&old[i].text).or_default().push(i);
    }
    let mut old_moved = vec![false; old.len()];
    let mut new_moved = vec![false; new.len()];
    for j in ops.iter().flat_map(DiffOp::new_range) {
        if let Some(i) = deleted.get_mut(new[j].text.as_str()).and_then(Vec::pop) {
            old_moved[i] = true;
            new_moved[j] = true;
        }
    }

    let unmoved = |paragraphs: &[&'a Text], moved: &[bool], range: Range<usize>| -> Vec<&'a Text> {
        range
            .filter(|&i| !moved[i])
            .map(|i| paragraphs[i])
            .collect()
    };
    let mut change = Change::default();
    for op in &ops {
        change += reworded(
            &unmoved(old, &old_moved, op.old_range()),
            &unmoved(new, &new_moved, op.new_range()),
        );
    }
    change
}

/// A run of changed paragraphs, diffed word by word when it has paragraphs
/// on both sides.
fn reworded(old: &[&Text], new: &[&Text]) -> Change {
    let words = |paragraphs: &[&Text]| paragraphs.iter().map(|p| p.words).sum::<u32>();
    if old.is_empty() || new.is_empty() {
        return Change {
            added: words(new),
            cut: words(old),
        };
    }

    fn split<'a>(paragraphs: &[&'a Text]) -> Vec<&'a str> {
        let words: Vec<&str> = paragraphs
            .iter()
            .flat_map(|p| p.text.unicode_words())
            .collect();
        debug_assert_eq!(
            words.len(),
            paragraphs.iter().map(|p| p.words as usize).sum::<usize>()
        );
        words
    }
    let (old, new) = (split(old), split(new));
    // Diffed as small integers: comparing them is cheaper than comparing
    // the words.
    fn intern<'a>(words: &[&'a str], ids: &mut HashMap<&'a str, u32>) -> Vec<u32> {
        words
            .iter()
            .map(|&word| {
                let next = ids.len() as u32;
                *ids.entry(word).or_insert(next)
            })
            .collect()
    }
    let mut ids = HashMap::new();
    let (old, new) = (intern(&old, &mut ids), intern(&new, &mut ids));
    let ops = capture_diff_slices(Algorithm::Histogram, &old, &new);

    let mut change = Change {
        added: new.len() as u32,
        cut: old.len() as u32,
    };
    for (i, op) in ops.iter().enumerate() {
        if let DiffOp::Equal { len, .. } = *op {
            let between_changes = i > 0 && i + 1 < ops.len();
            if len >= MIN_KEPT || !between_changes {
                change.added -= len as u32;
                change.cut -= len as u32;
            }
        }
    }
    change
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::new(text, &TextFilter::new(), None)
    }

    fn change(old: &str, new: &str) -> Change {
        let (old, new) = (version(old), version(new));
        let old: Vec<&Text> = old.paragraphs().iter().collect();
        let new: Vec<&Text> = new.paragraphs().iter().collect();
        diff(&old, &new)
    }

    #[test]
    fn unchanged_text_adds_and_cuts_nothing() {
        let text = "# One\n\nAria rode out.\n\nThe storm broke.\n";
        assert_eq!(change(text, text), Change::default());
    }

    #[test]
    fn whole_paragraphs_count_in_full() {
        let old = "first para here\n\nsecond one\n";
        let new = "first para here\n\nsecond one\n\na brand new third\n";
        assert_eq!(change(old, new), Change { added: 4, cut: 0 });
        assert_eq!(change(new, old), Change { added: 0, cut: 4 });
    }

    #[test]
    fn an_edit_inside_a_paragraph_counts_only_the_changed_words() {
        // The Yeneth → Ismay rename: one word out, one in, not the paragraph.
        let old = "But Yeneth did not. Aria reached out toward the girl and caught the reins.\n";
        let new = "But Ismay did not. Aria reached out toward the girl and caught the reins.\n";
        assert_eq!(change(old, new), Change { added: 1, cut: 1 });
    }

    #[test]
    fn a_beat_sheet_replaced_by_a_scene_counts_both_in_full() {
        let old = "# Scene\n\nAria meets the khan. She refuses him.\n\nThen the storm.\n";
        let new = "# Scene\n\nThe khan received her in the tent. She said no, and he laughed.\n\n\
                   By nightfall the storm had come.\n";
        let (old_words, new_words) = (version(old).words, version(new).words);
        // "the khan", "She", "the storm" all recur, but only by chance.
        assert_eq!(
            change(old, new),
            Change {
                added: new_words,
                cut: old_words
            }
        );
    }

    #[test]
    fn moved_paragraphs_count_as_neither() {
        let old = "# One\n\nfirst scene words\n\nsecond scene here\n\n# Two\n\nlast bit\n";
        let new = "# One\n\nsecond scene here\n\n# Two\n\nlast bit\n\nfirst scene words\n";
        assert_eq!(change(old, new), Change::default());

        // Moved and then a new paragraph written beside it: only the new one.
        let new = "# Two\n\nlast bit\n\nfirst scene words\n\nbrand new\n\nsecond scene here\n";
        assert_eq!(change(old, new), Change { added: 2, cut: 0 });
    }

    #[test]
    fn a_renamed_file_that_sorts_elsewhere_moves_nothing() {
        let (a, b, c) = (
            version("aaa aaa\n"),
            version("bbb\n"),
            version("ccc cc c\n"),
        );
        fn paragraphs<'a>(files: &[&'a Version]) -> Vec<&'a Text> {
            files.iter().flat_map(|file| file.paragraphs()).collect()
        }
        // chapter.05 → defunct.05: same text, later in path order.
        let change = diff(&paragraphs(&[&a, &b, &c]), &paragraphs(&[&a, &c, &b]));
        assert_eq!(change, Change::default());
    }

    #[test]
    fn a_long_shared_run_counts_as_kept() {
        let old = "one two three four five six\n";
        let new = "zero one two three four five six seven\n";
        assert_eq!(change(old, new), Change { added: 2, cut: 0 });
    }

    #[test]
    fn a_comment_splitting_a_word_counts_like_the_table() {
        // Counted as two pieces ("half" and "way"), so diffed as two words.
        let old = "half<!-- x -->way there\n";
        let new = "half<!-- x -->way there and back\n";
        assert_eq!(version(old).words, 3);
        assert_eq!(change(old, new), Change { added: 2, cut: 0 });
    }

    #[test]
    fn net_always_matches_the_totals() {
        let texts = [
            "",
            "# A\n\none two three\n",
            "# A\n\none two three\n\nfour five\n",
            "# A\n\nfour five six seven\n\none two\n",
            "# B\n\nsomething else entirely, with words.\n",
        ];
        for old in texts {
            for new in texts {
                let net = i64::from(version(new).words) - i64::from(version(old).words);
                assert_eq!(change(old, new).net(), net, "{old:?} → {new:?}");
            }
        }
    }

    #[test]
    fn the_section_is_get_headings_match_and_its_subsections() {
        let text = "intro\n\n# One\n\na b\n\n## Sub\n\nc d e\n\n# Two\n\nf\n";
        let version = Version::new(text, &TextFilter::new(), Some("ONE"));
        assert_eq!(version.section_words(), Some(5));

        // `get_heading` prefers a shallower match over an earlier one.
        let text = "# A\n\nx\n\n## Target deep\n\ny y\n\n# Target\n\nz z z\n";
        let version = Version::new(text, &TextFilter::new(), Some("TARGET"));
        assert_eq!(version.section_words(), Some(3));

        let version = Version::new(text, &TextFilter::new(), Some("NOPE"));
        assert_eq!(version.section_words(), None);
    }
}
