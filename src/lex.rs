use std::io::{BufRead, Result};
use regex::Regex;

#[derive(Debug)]
pub enum Lexeme {
    Heading(String),
    Paragraph(String),
}

pub struct Lexemes<'a, T> {
    lexer: &'a Lexer,
    text: T,
}

impl<'a, T: BufRead> Iterator for Lexemes<'a, T> {
    type Item = Result<Lexeme>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut buf = String::new();
            match self.text.read_line(&mut buf) {
                Err(e) => return Some(Err(e)),
                Ok(0) => return None,
                Ok(_) => if let Some(lexeme) = self.lexer.lex_string(buf) {
                    return Some(Ok(lexeme));
                } else {
                    continue;
                },
            }
        }
    }
}

pub struct Lexer {
    comments: Regex,
}

impl Lexer {
    pub fn new() -> Self {
        let comments = Regex::new(r#"<[^<]*>"#).expect("Failed to initialize comment pattern");

        Self { comments }
    }

    pub fn lexemes(&self, text: impl BufRead) -> Lexemes<impl BufRead> {
        Lexemes { lexer: self, text }
    }

    fn lex_string(&self, s: String) -> Option<Lexeme> {
        let s = self.comments.replace(s.as_ref(), "");

        if s.trim().is_empty() {
            return None;
        }

        if s.starts_with('#') {
            return Some(Lexeme::Heading(s.into()));
        }

        if is_valid_line(&s) {
            return Some(Lexeme::Paragraph(s.into()));
        }

        None
    }
}

// Ignore anything that's not text. Text can start with these legal characters.
fn is_valid_line(s: &str) -> bool {
    !s.is_empty() && s.starts_with(|c| {
        c == '"'                // Dialog
        || c == '.'             // Ellipsis
        || c == '*'             // Italics
        || c.is_alphabetic()    // Letters
    })
}
