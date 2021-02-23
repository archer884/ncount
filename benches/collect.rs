// This benchmark has been left in so that anyone who gets the bright idea to
// use a regular expression to perform word counting will know better. In a
// word: DON'T. Regexes are about 50% slower than the compound split.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use regex::Regex;

static TEXT: &str = include_str!("../resources/sample.md");

pub trait WordCount {
    fn word_count(&self, s: &str) -> u32;
}

pub struct DefaultWordCount;

impl WordCount for DefaultWordCount {
    fn word_count(&self, s: &str) -> u32 {
        // Words are usually separated by spaces, but they
        // could be separated by m-dashes instead. We do not
        // count hyphenated words as two words.
        s.split_whitespace().flat_map(|s| s.split("---")).count() as u32
    }
}

struct RegexWordCount {
    pattern: Regex,
}

impl RegexWordCount {
    fn new() -> Self {
        Self {
            pattern: Regex::new(r"\w+('\w+)*").unwrap(),
        }
    }
}

impl WordCount for RegexWordCount {
    fn word_count(&self, s: &str) -> u32 {
        self.pattern.find_iter(s).count() as u32
    }
}

fn benchmarks(c: &mut Criterion) {
    let compound_split = DefaultWordCount;
    let regex_pattern = RegexWordCount::new();

    c.bench_function("compound split", |b| {
        b.iter(|| black_box(compound_split.word_count(TEXT)));
    });

    c.bench_function("regex pattern", |b| {
        b.iter(|| black_box(regex_pattern.word_count(TEXT)));
    });
}

criterion_group!(collect, benchmarks);
criterion_main!(collect);
