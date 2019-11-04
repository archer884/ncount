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

        for path in read_paths(opt.paths())? {
            apply_path(&path, &mut collector)?;
        }

        println!("{}", collector.as_table());

        Ok(())
    }
}

fn apply_path(path: &Path, collector: &mut Collector) -> Result<()> {
    use std::fs;

    let filename = path.file_name().and_then(|name| name.to_str());
    let text = fs::read_to_string(path)?;

    apply_str(filename, &text, collector)
}

fn apply_str(filename: Option<&str>, text: &str, collector: &mut Collector) -> Result<()> {
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
        None => {
            if let Some(filename) = filename {
                collector.push_with_heading(filename, stats)
            } else {
                collector.push(stats)
            }
        }

        Some(heading) => collector.push_with_heading(heading, stats),
    }

    Ok(())
}

fn heading_name(s: &str) -> String {
    s.trim_start_matches(|x: char| x == '#' || x.is_whitespace())
        .to_owned()
}

fn read_paths<'a>(candidates: impl Iterator<Item = &'a str>) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for s in candidates {
        let path: &Path = s.as_ref();
        match path.metadata() {
            Ok(meta) => {
                if meta.file_type().is_file() {
                    paths.push(path.into());
                } else {
                    let walker = walkdir::WalkDir::new(path)
                        .contents_first(true)
                        .into_iter()
                        .filter_entry(|entry| {
                            entry
                                .metadata()
                                .map(|meta| meta.file_type().is_file())
                                .unwrap_or_default()
                        })
                        .filter_map(|entry| entry.ok().map(|entry| entry.path().into()));
                    paths.extend(walker);
                }
            }

            // Possible glob pattern. If globbing fails, just return the existing error instead
            // of the glob pattern error.
            Err(e) => {
                let globbed_paths = match glob::glob(s) {
                    Ok(g) => g,
                    Err(_) => return Err(e.into()),
                };

                let globbed_paths =
                    globbed_paths
                        .filter_map(|item| item.ok())
                        .filter(|candidate| {
                            candidate
                                .metadata()
                                .map(|meta| meta.file_type().is_file())
                                .unwrap_or_default()
                        });
                paths.extend(globbed_paths);
            }
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

        apply_str(None, TEXT, &mut collector).unwrap();

        let Stats {
            word_count,
            paragraph_count,
            ..
        } = collector.overall_stats();

        assert_eq!(321, word_count, "{:?}", collector.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", collector.overall_stats());
    }

    #[test]
    fn can_parse_numbers() {
        let mut collector = Collector::new();
        apply_str(None, "He drove a V-6", &mut collector).unwrap();
    }

    #[test]
    fn can_parse_times() {
        let mut collector = Collector::new();
        apply_str(None, "It was 10:30", &mut collector).unwrap();
    }
}
