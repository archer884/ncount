use crate::{
    collector::{Collector, Stats},
    error::Result,
    opt::Opt,
};
use std::path::{Path, PathBuf};

pub struct Application;

impl Application {
    pub fn run(&self, opt: &Opt) -> Result<()> {
        let mut collector = Collector::new();

        for path in read_paths(opt.path())? {
            apply_path(&path, &mut collector)?;
        }

        println!("{}", collector.as_table());

        Ok(())
    }
}

fn apply_path(path: &Path, collector: &mut Collector) -> Result<()> {
    use std::fs;

    let text = fs::read_to_string(path)?;
    apply_str(&text, collector)
}

fn apply_str(text: &str, collector: &mut Collector) -> Result<()> {
    use crate::parse::{MarkdownParser, Rule};
    use pest::Parser;

    let document = MarkdownParser::parse(Rule::Document, &text)?;

    let mut heading = None;
    let mut stats = Stats::default();

    for element in document.flatten() {
        match element.as_rule() {
            Rule::Title => match heading.take() {
                None => heading = Some(heading_name(element.as_str())),
                Some(previous_heading) => {
                    collector.push_with_heading(previous_heading, stats);
                    stats = Stats::default();
                }
            },
            Rule::Paragraph => stats.push(element.into_inner().count() as u32),

            // We are uninterested in other parse events because we'll get
            // the word count via the inner elements of each paragraph.
            _ => (),
        }
    }

    match heading {
        None => collector.push(stats),
        Some(heading) => collector.push_with_heading(heading, stats),
    }

    Ok(())
}

fn heading_name(s: &str) -> String {
    s.trim_start_matches(|x: char| x == '#' || x.is_whitespace())
        .to_owned()
}

fn read_paths(path: &Path) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

    let mut paths = Vec::new();

    for entry in WalkDir::new(path).into_iter() {
        let entry = entry?;

        if entry.file_type().is_file() {
            paths.push(entry.into_path());
        }
    }

    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::apply_str;
    use crate::collector::{Collector, Stats};

    static TEXT: &str = include_str!("../resources/sample.md");

    #[test]
    fn stats_are_correct() {
        let mut collector = Collector::new();

        apply_str(TEXT, &mut collector).unwrap();

        let Stats {
            word_count,
            paragraph_count,
            ..
        } = collector.overall_stats();

        assert_eq!(321, word_count, "{:?}", collector.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", collector.overall_stats());
    }
}
