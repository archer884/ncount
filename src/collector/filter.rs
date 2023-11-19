use regex::Regex;

#[derive(Clone, Debug)]
pub struct TagFilter {
    tag: Regex,
}

impl TagFilter {
    fn new() -> Self {
        Self {
            tag: Regex::new("<note|<!--").unwrap(),
        }
    }

    pub fn filter(&self, text: &str) -> String {
        let mut text = text;
        let mut state = false;
        let mut result = String::with_capacity(text.len());

        while !text.is_empty() {
            if !state {
                if let Some(loc) = self.tag.find(text) {
                    state = true;
                    result.push_str(&text[..loc.start()]);
                    text = &text[loc.end()..];
                } else {
                    result.push_str(text);
                    return result;
                }
            } else if let Some(idx) = text.find('>') {
                state = false;
                text = &text[idx + 1..];
            }
        }

        result
    }
}

impl Default for TagFilter {
    fn default() -> Self {
        Self::new()
    }
}
