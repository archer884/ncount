use regex::{Match, Regex};

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
            tag: Regex::new(r"<note.+?>|<!--(.|\n)+?-->|^\[\^[^\[]+\]:.+$|\[\^[^\[]+\]").unwrap(),
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


// FIXME: I think this kind of iterative solution will only work insofar as we never split a
// paragraph, which is just not tenable. We're bound to split one eventually, and in fact I'm sure
// footnotes cause exactly that to happen. We should just go back to the regex.replace strategy.

pub struct FilteredText<'a> {
    filter: &'a TextFilter,
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
            self.text = self.text[m.end()..].trim();
            Some(result)
        } else {
            let result = self.text;
            self.text = "";
            Some(result)
        }
    }
}
