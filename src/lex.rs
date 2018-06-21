use regex::Regex;
use split::{Splitter, Words};
use std::collections::VecDeque;
use std::io::{BufRead, Result};

#[derive(Debug, Eq, PartialEq)]
pub enum Lexeme<'content> {
    Comment(&'content str),
    Heading(&'content str),
    Text(&'content str),
    Whitespace(&'content str),
}

enum BoundKind {
    Comment,
    Heading,
    Text,
    Whitespace,
}

struct Bound {
    kind: BoundKind,
    left: usize,
    right: usize,
}

pub struct Lexer {
    comments: Regex,
    splitter: Splitter,
}

impl Lexer {
    pub fn new() -> Self {
        let comments = Regex::new(r#"<[^<]*>"#).expect("Failed to initialize comment pattern");
        let splitter = Splitter::new();

        Self { comments, splitter }
    }

    pub fn lexemes<S: BufRead>(&self, stream: S) -> Lexemes<S> {
        Lexemes::new(self, stream)
    }

    pub fn words<'text, 'lexer: 'text>(&'lexer self, s: &'text str) -> impl Words<'text> {
        self.splitter.words(s)
    }

    fn map_buffer(&self, slices: &mut VecDeque<Bound>, s: &str) {
        // Headings
        if s.starts_with('#') {
            slices.push_back(Bound {
                kind: BoundKind::Heading,
                left: 0,
                right: s.len(),
            });
            return;
        }

        // Whitespace
        if s.chars().all(char::is_whitespace) {
            slices.push_back(Bound {
                kind: BoundKind::Whitespace,
                left: 0,
                right: s.len(),
            });
            return;
        }

        // Comment
        if !is_valid_line(s) {
            slices.push_back(Bound {
                kind: BoundKind::Comment,
                left: 0,
                right: s.len(),
            });
            return;
        }

        // Text
        let mut idx = 0;
        for location in self.comments.find_iter(s) {
            if location.start() == idx {
                slices.push_back(Bound {
                    kind: BoundKind::Comment,
                    left: location.start(),
                    right: location.end(),
                });
            } else {
                slices.push_back(Bound {
                    kind: BoundKind::Text,
                    left: idx,
                    right: location.start(),
                });
                slices.push_back(Bound {
                    kind: BoundKind::Comment,
                    left: location.start(),
                    right: location.end(),
                });
            }
            idx = location.end();
        }

        match idx {
            0 => slices.push_back(Bound {
                kind: BoundKind::Text,
                left: 0,
                right: s.len(),
            }),

            idx => slices.push_back(Bound {
                kind: BoundKind::Text,
                left: idx,
                right: s.len(),
            }),
        }
    }
}

pub struct Lexemes<'lexer, Stream> {
    lexer: &'lexer Lexer,
    stream: Stream,
    buffer: String,
    slices: VecDeque<Bound>,
}

impl<'lexer, S: BufRead> Lexemes<'lexer, S> {
    pub fn new(lexer: &'lexer Lexer, stream: S) -> Lexemes<'lexer, S> {
        Lexemes {
            lexer,
            stream,
            buffer: String::new(),
            slices: VecDeque::new(),
        }
    }

    pub fn next<'caller, 'lexemes: 'caller>(&'lexemes mut self) -> Option<Result<Lexeme<'caller>>> {
        if let Some(bound) = self.slices.pop_front() {
            return Some(Ok(bounded_lexeme(bound, &self.buffer)));
        }

        match self.fill_buffer() {
            Ok(0) => None,
            Ok(_non_zero_length) => self.next(),
            Err(e) => Some(Err(e)),
        }
    }

    fn fill_buffer(&mut self) -> Result<usize> {
        self.buffer.clear();
        match self.stream.read_line(&mut self.buffer) {
            Ok(0) => Ok(0),
            Ok(len) => {
                self.lexer.map_buffer(&mut self.slices, &self.buffer);
                Ok(len)
            }
            error => error,
        }
    }
}

fn bounded_lexeme(Bound { kind, left, right }: Bound, s: &str) -> Lexeme {
    use self::BoundKind::*;

    let slice = &s[left..right];
    match kind {
        Comment => Lexeme::Comment(slice),
        Heading => Lexeme::Heading(slice),
        Text => Lexeme::Text(slice),
        Whitespace => Lexeme::Whitespace(slice),
    }
}

fn is_valid_line(s: &str) -> bool {
    s.starts_with(|c: char| {
        c == '"'             // Dialog
        || c == '.'          // Ellipsis
        || c == '*'          // Italics
        || c.is_alphabetic() // Letters
    })
}

#[cfg(test)]
mod tests {
    use lex::{Lexeme, Lexer};
    use std::io::Cursor;

    #[test]
    fn can_create_lexer() {
        Lexer::new();
    }

    #[test]
    fn lexemes_are_correct() {
        let text = "This is a text <!-- with comments --> in the middle.";
        let lexer = Lexer::new();
        let mut lexemes = lexer.lexemes(Cursor::new(text));

        assert_eq!(
            Lexeme::Text("This is a text "),
            lexemes.next().expect("was none").expect("was error"),
        );

        assert_eq!(
            Lexeme::Comment("<!-- with comments -->"),
            lexemes.next().expect("was none").expect("was error"),
        );

        assert_eq!(
            Lexeme::Text(" in the middle."),
            lexemes.next().expect("was none").expect("was error"),
        );
    }
}
