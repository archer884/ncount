use std::iter::Peekable;

use compact_str::CompactString;
use regex::{Matches, Regex};

use crate::document::count_words;

pub struct TextFilter {
    tag: Regex,
}

impl TextFilter {
    pub fn new() -> Self {
        // Footnotes:
        // ^\[\^[^\[]+\]:.+$|\[\^[^\[]+\]
        // HTML comment:
        // <!--(.|\n)+?-->
        // Inline notes:
        // <note.+?>
        Self {
            tag: Regex::new(r"<note.+?>|<!--(.|\n)+?-->|(?m:^\[\^[^\[]+\]:.+$)|\[\^[^\[]+\]")
                .unwrap(),
        }
    }

    /// Lexes `s` into a stream of heading/paragraph events, skipping
    /// comments/footnotes/notes as it goes rather than materializing a
    /// separate cleaned copy of the text first. A comment or footnote
    /// landing in the middle of a line does not split that line into two
    /// paragraphs: text is bundled together up to the next real
    /// (non-removed) line break.
    pub fn lex<'a>(&'a self, s: &'a str) -> impl Iterator<Item = LineEvent> + 'a {
        Lines {
            chunks: Chunks {
                text: s,
                matches: self.tag.find_iter(s).peekable(),
                pos: 0,
            },
            current: None,
            pending_mode: LineMode::Undecided,
            pending_non_whitespace: false,
        }
    }
}

/// One heading or one paragraph, already fully assembled from however many
/// chunks it took to get there.
#[derive(Debug, Clone, PartialEq)]
pub enum LineEvent {
    Heading(CompactString, i32),
    Paragraph(u32),
}

/// Walks `text`, skipping over `matches`, yielding the surviving text
/// between them as a single slice each — unlike a per-line lexer, a chunk
/// can span many real lines; it's only ever split at a removed span, so
/// there's one chunk boundary per match rather than one per line. Cost here
/// scales with how many comments/footnotes exist (typically a handful),
/// not with how many lines the file has (typically thousands).
struct Chunks<'a> {
    text: &'a str,
    matches: Peekable<Matches<'a, 'a>>,
    pos: usize,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(m) = self.matches.peek() {
            if m.start() == self.pos {
                self.pos = self.matches.next().unwrap().end();
            } else {
                break;
            }
        }

        if self.pos >= self.text.len() {
            return None;
        }

        let gap_end = self
            .matches
            .peek()
            .map(|m| m.start())
            .unwrap_or(self.text.len());
        let gap = &self.text[self.pos..gap_end];
        self.pos = gap_end;
        Some(gap)
    }
}

enum LineMode {
    Undecided,
    Heading(CompactString),
    Paragraph(u32),
}

/// Bundles a `Chunks` stream into complete heading/paragraph events, one per
/// real line, using ordinary `str::lines()` over the bulk of each chunk (a
/// well-optimized std primitive) and only doing the "does this paragraph
/// continue into the next chunk?" check at the two chunk boundaries. That
/// check comes down to one thing: whether the chunk ends with `\n`. If it
/// does, its last line was already cleanly terminated before the next match
/// even started, so the following chunk starts a genuinely new line. If it
/// doesn't, the match spliced two textual halves together inline (a comment
/// sitting mid-sentence), so the next chunk's first line must be merged
/// onto this chunk's dangling last line rather than treated separately.
struct Lines<'a> {
    chunks: Chunks<'a>,
    current: Option<(Peekable<std::str::Lines<'a>>, bool)>,
    pending_mode: LineMode,
    pending_non_whitespace: bool,
}

impl Lines<'_> {
    fn absorb(&mut self, line: &str) {
        if !line.trim().is_empty() {
            self.pending_non_whitespace = true;
        }
        match &mut self.pending_mode {
            LineMode::Undecided => {
                self.pending_mode = if line.starts_with('#') {
                    LineMode::Heading(CompactString::from(line))
                } else {
                    LineMode::Paragraph(count_words(line))
                };
            }
            LineMode::Heading(buf) => buf.push_str(line),
            LineMode::Paragraph(count) => *count += count_words(line),
        }
    }

    fn take_pending(&mut self) -> Option<LineEvent> {
        let non_whitespace = std::mem::take(&mut self.pending_non_whitespace);
        let mode = std::mem::replace(&mut self.pending_mode, LineMode::Undecided);
        if !non_whitespace {
            return None;
        }
        Some(match mode {
            LineMode::Heading(raw) => {
                let level = raw.bytes().take_while(|&b| b == b'#').count() as i32;
                let text = raw.trim_start_matches('#').trim();
                LineEvent::Heading(CompactString::from(text), level)
            }
            LineMode::Paragraph(count) => LineEvent::Paragraph(count),
            LineMode::Undecided => unreachable!("non_whitespace implies mode was set"),
        })
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = LineEvent;

    fn next(&mut self) -> Option<LineEvent> {
        loop {
            if self.current.is_none() {
                loop {
                    match self.chunks.next() {
                        Some(chunk) if !chunk.is_empty() => {
                            let ends_with_nl = chunk.ends_with('\n');
                            self.current = Some((chunk.lines().peekable(), ends_with_nl));
                            break;
                        }
                        Some(_empty) => continue,
                        None => return self.take_pending(),
                    }
                }
            }

            // Pull one line out, ending the borrow on `self.current` before
            // calling `absorb`/`take_pending` (both take `&mut self`).
            let (line, is_last, ends_with_nl) = {
                let (lines, ends_with_nl) = self.current.as_mut().unwrap();
                match lines.next() {
                    Some(line) => (line, lines.peek().is_none(), *ends_with_nl),
                    None => {
                        self.current = None;
                        continue;
                    }
                }
            };

            if is_last {
                self.current = None;
            }

            self.absorb(line);

            if !is_last || ends_with_nl {
                if let Some(event) = self.take_pending() {
                    return Some(event);
                }
                // blank line; keep looping (chunk exhausted or not)
            }
            // else: last line of a chunk that doesn't end in `\n` — it's
            // dangling, so leave it pending and merge with the next chunk.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(s: &str) -> Vec<LineEvent> {
        TextFilter::new().lex(s).collect()
    }

    fn heading(text: &str, level: i32) -> LineEvent {
        LineEvent::Heading(CompactString::from(text), level)
    }

    #[test]
    fn strips_html_comments() {
        assert_eq!(
            lex("before <!-- hidden --> after"),
            vec![LineEvent::Paragraph(2)]
        );
    }

    #[test]
    fn multiline_comment_merges_surrounding_text_into_one_paragraph() {
        // The whole point of this rewrite: a comment spanning several
        // source lines must not fragment the paragraph it interrupts.
        assert_eq!(
            lex("word1 <!-- line one\nline two\nline three --> word2"),
            vec![LineEvent::Paragraph(2)]
        );
    }

    #[test]
    fn strips_footnote_definition_lines() {
        let input = "text[^note]\n\n[^note]: This whole line is a definition.\n\nmore text";
        assert_eq!(
            lex(input),
            vec![LineEvent::Paragraph(1), LineEvent::Paragraph(2)]
        );
    }

    #[test]
    fn strips_inline_footnote_references() {
        assert_eq!(lex("word[^1] and word[^2]"), vec![LineEvent::Paragraph(3)]);
    }

    #[test]
    fn strips_inline_notes() {
        let input = r#"before <note tag="foo" comment="bar"> after"#;
        assert_eq!(lex(input), vec![LineEvent::Paragraph(2)]);
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(
            lex("Don't stop, it's co-authored work."),
            vec![LineEvent::Paragraph(6)]
        );
    }

    #[test]
    fn heading_interrupted_by_comment_reassembles_text() {
        assert_eq!(
            lex("# Chapter <!-- todo: rename --> One"),
            vec![heading("Chapter  One", 1)]
        );
    }

    #[test]
    fn heading_hidden_entirely_behind_a_leading_comment_still_detected() {
        assert_eq!(
            lex("<!-- todo --># Real heading"),
            vec![heading("Real heading", 1)]
        );
    }

    #[test]
    fn blank_lines_are_skipped_not_emitted_as_empty_paragraphs() {
        assert_eq!(
            lex("first\n\n\n\nsecond"),
            vec![LineEvent::Paragraph(1), LineEvent::Paragraph(1)]
        );
    }

    #[test]
    fn punctuation_only_line_still_counts_as_a_zero_word_paragraph() {
        // Distinct from a fully-removed line: "---" survives filtering with
        // non-whitespace content, it just happens to contain no words.
        assert_eq!(
            lex("real words\n\n---\n\nmore words"),
            vec![
                LineEvent::Paragraph(2),
                LineEvent::Paragraph(0),
                LineEvent::Paragraph(2),
            ]
        );
    }

    #[test]
    fn multiple_headings_and_levels() {
        assert_eq!(
            lex("# One\n\ntext\n\n## Two\n\nmore text"),
            vec![
                heading("One", 1),
                LineEvent::Paragraph(1),
                heading("Two", 2),
                LineEvent::Paragraph(2),
            ]
        );
    }

    #[test]
    fn final_line_without_trailing_newline_is_still_emitted() {
        assert_eq!(lex("no trailing newline"), vec![LineEvent::Paragraph(3)]);
    }
}
