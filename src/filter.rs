use regex::Regex;

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

    pub fn filter_text<'a>(&'a self, s: &'a str) -> impl AsRef<str> + 'a {
        let mut t = libsw::Sw::new();
        let result = {
            let _guard = t.guard();
            self.tag.replace_all(s, "")
        };
        tracing::debug!(elapsed = ?t.elapsed(), "tags replaced");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(s: &str) -> String {
        TextFilter::new().filter_text(s).as_ref().to_string()
    }

    #[test]
    fn strips_html_comments() {
        assert_eq!(filter("before <!-- hidden --> after"), "before  after");
    }

    #[test]
    fn strips_multiline_html_comments() {
        assert_eq!(filter("before <!-- line one\nline two --> after"), "before  after");
    }

    #[test]
    fn strips_footnote_definition_lines() {
        let input = "text[^note]\n\n[^note]: This whole line is a definition.\n\nmore text";
        assert_eq!(filter(input), "text\n\n\n\nmore text");
    }

    #[test]
    fn strips_inline_footnote_references() {
        assert_eq!(filter("word[^1] and word[^2]"), "word and word");
    }

    #[test]
    fn strips_inline_notes() {
        let input = r#"before <note tag="foo" comment="bar"> after"#;
        assert_eq!(filter(input), "before  after");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let input = "Don't stop, it's co-authored work.";
        assert_eq!(filter(input), input);
    }
}
