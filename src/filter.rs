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
