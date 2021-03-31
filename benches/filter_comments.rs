use criterion::{black_box, criterion_group, criterion_main, Criterion};
use regex::Regex;

static TEXT: &str = include_str!("../resources/sample.md");

fn filter_comments(text: &str) -> String {
    let mut text = text;
    let mut state = false;
    let mut result = String::with_capacity(text.len());

    while !text.is_empty() {
        if !state {
            if let Some(idx) = text.find("<!--") {
                state = true;
                result.push_str(&text[..idx]);
                text = &text[idx..];
            } else {
                result.push_str(text);
                return result;
            }
        } else if let Some(idx) = text.find("-->") {
            state = false;
            text = &text[(idx + 3)..];
        }
    }

    result
}

fn benchmarks(c: &mut Criterion) {
    let pattern = Regex::new(r#"(?s)<!--.*?-->"#).unwrap();

    c.bench_function("filter", |b| {
        b.iter(|| black_box(filter_comments(black_box(TEXT))));
    });

    c.bench_function("regex", |b| {
        b.iter(|| black_box(pattern.replace_all(black_box(TEXT), "")));
    });
}

criterion_group!(filter, benchmarks);
criterion_main!(filter);
