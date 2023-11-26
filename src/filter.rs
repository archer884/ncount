use regex::{Match, Regex};

pub struct TagFilter {
    tag: Regex,
}

impl TagFilter {
    pub fn new() -> Self {
        Self {
            tag: Regex::new("<note|<!--").unwrap(),
        }
    }

    pub fn filter_text<'a>(&'a self, s: &'a str) -> FilteredText<'a> {
        FilteredText {
            filter: self,
            text: s,
        }
    }

    #[inline]
    fn next_tag<'a>(&self, s: &'a str) -> Option<Match<'a>> {
        self.tag.find(s)
    }
}

pub struct FilteredText<'a> {
    filter: &'a TagFilter,
    text: &'a str,
}

impl<'a> Iterator for FilteredText<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.text.is_empty() {
            return None;
        }

        if let Some(m) = self.filter.next_tag(self.text) {
            let result = self.text[..m.start()].trim();
            self.text = advance_text(&self.text[m.start()..]);
            Some(result)
        } else {
            let result = self.text;
            self.text = "";
            Some(result)
        }
    }
}

#[inline]
fn advance_text(text: &str) -> &str {
    if let Some(end) = text.find('>') {
        text[end + 1..].trim()
    } else {
        ""
    }
}