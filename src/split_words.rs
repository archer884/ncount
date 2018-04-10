pub trait SplitWords {
    fn split_words(&self) -> WordIterator;
}

impl<T: AsRef<str>> SplitWords for T {
    fn split_words(&self) -> WordIterator {
        WordIterator {
            idx: 0,
            source: self.as_ref(),
        }
    }
}

pub struct WordIterator<'source> {
    idx: usize,
    source: &'source str,
}

impl<'source> Iterator for WordIterator<'source> {
    type Item = &'source str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.source.len() {
            return None;
        }

        let haystack = &self.source[self.idx..];
        match haystack.find(is_break_char) {
            None => {
                self.idx = self.source.len();
                if !haystack.is_empty() {
                    Some(haystack)
                } else {
                    None
                }
            }

            Some(break_idx) => {
                let word_candidate = &haystack[..break_idx];
                self.idx += break_idx + 1;
                if !word_candidate.is_empty() {
                    Some(word_candidate)
                } else {
                    // fixme: this could cause a stack overflow when faced with a zillion dashes.
                    self.next()
                }
            }
        }
    }
}

#[inline] // Is this really necessary?
fn is_break_char(c: char) -> bool {
    // FIXME: the colon (:) counts as ascii punctuation but is not a true break character when 
    // it appears within a time, e.g. "3:45 PM." For now, to correct counting of times, here is
    // a hack:

    c.is_whitespace() || (
        c.is_ascii_punctuation() && (c != ':' && c != '\'')
    )
}

#[cfg(test)]
mod tests {
    use split_words::SplitWords;

    // Real text from my novel--one of only two paragraphs in the first scene containing an mdash.
    static TEXT: &str = "Though the island was without paths, Grier tried never to follow the \
    same path twice. Breathless, she paused for an moment on an outcrop of bald stone at the \
    brow of a hill. Warmed by her run, she pulled off her hoodie and tied it around her waist, \
    and she took another instant to get her bearings. There: the dead tree she had passed \
    yesterday--a wizened hulk, stripped of bark and gray with age--waited there, pointing to \
    the right. She had gone left yesterday.";

    static TEXT_WITH_ABBREVIATION: &str = "Don't look now, but this may break.";

    #[test]
    fn count_is_correct() {
        let count = TEXT.split_words().count();

        // Expected count provided by Word.
        assert_eq!(86, count);
    }

    #[test]
    fn abbreviations_are_counted_correctly() {
        let count = TEXT_WITH_ABBREVIATION.split_words().count();
        assert_eq!(7, count);
    }

    #[test]
    fn coffee_case() {
        let text = r#""I looked at the schedule, you know," she said on their way back from the university. "We can stop at this cafe, have a snack, and take the next bus at 3:45.""#;
        let count = text.split_words().count();
        assert_eq!(32, count);
    }
}
