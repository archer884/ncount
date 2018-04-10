use regex::Regex;

pub struct Splitter {
    pattern: Regex,
}

impl Splitter {
    pub fn new() -> Self {
        let pattern = Regex::new(r#"[\w':]+"#).unwrap();
        
        Self { pattern }
    }

    pub fn words<'text, 'splitter: 'text>(&'splitter self, s: &'text str) -> Words<'text> {
        let words = self.pattern
            .captures_iter(s)
            .map(|capture| capture.get(0).unwrap().as_str());

        Words { source: Box::new(words) }
    }
}

pub struct Words<'text> {
    source: Box<Iterator<Item = &'text str> + 'text>,
}

impl<'text> Iterator for Words<'text> {
    type Item = &'text str;

    fn next(&mut self) -> Option<Self::Item> {
        self.source.next()
    }
}

#[cfg(test)]
mod tests {
    use split::Splitter;

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
        let splitter = Splitter::new();
        let count = splitter.words(TEXT).count();
        assert_eq!(86, count);
    }

    #[test]
    fn abbreviations_are_counted_correctly() {
        let splitter = Splitter::new();
        let count = splitter.words(TEXT_WITH_ABBREVIATION).count();
        assert_eq!(7, count);
    }

    #[test]
    fn coffee_case() {
        let text = r#""I looked at the schedule, you know," she said on their way back from the university. "We can stop at this cafe, have a snack, and take the next bus at 3:45.""#;
        let splitter = Splitter::new();
        let count = splitter.words(text).count();
        assert_eq!(32, count);
    }
}
