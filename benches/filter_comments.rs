use criterion::{black_box, criterion_group, criterion_main, Criterion};
use paragraphs::Paragrapher;
use regex::Regex;

// static TEXT: &str = include_str!("../../../Documents/sacrifice-valkyrie/src/chapter.01.md");
static TEXT: &str = include_str!("../resources/sample.md");

fn filter_old(text: &str) -> String {
    let mut text = text;
    let mut state = false;
    let mut result = String::new();

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
        b.iter(|| black_box(filter_old(black_box(TEXT))));
    });

    c.bench_function("regex", |b| {
        b.iter(|| black_box(pattern.replace_all(black_box(TEXT), "")));
    });

    c.bench_function("paragraph", |b| {
        let paragrapher = Paragrapher::new();
        b.iter(|| black_box(paragrapher.paragraphs(black_box(TEXT)).count()));
    });
}

criterion_group!(filter, benchmarks);
criterion_main!(filter);
