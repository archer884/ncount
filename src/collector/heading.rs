#[derive(Clone, Debug)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

impl Heading {
    pub fn from_str(s: &str) -> Self {
        let text = s
            .trim_start_matches(|x: char| x == '#' || x.is_whitespace())
            .replace(|u: char| u == '*' || u == '_', "");

        Self {
            level: s.bytes().filter(|&u| u == b'#').count() as u8,
            text,
        }
    }
}
