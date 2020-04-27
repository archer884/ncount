use crate::{collector::Collector, opt::Opt};
use std::path::{Path, PathBuf};
use std::{fs, iter};

pub struct Application {
    options: Opt,
    collector: Collector,
}

impl Application {
    pub fn with_opts(options: Opt) -> Self {
        Application {
            options,
            collector: Collector::new(),
        }
    }

    pub fn run(&mut self) -> crate::Result<()> {
        let paths: Vec<_> = self.options.paths().flat_map(extract_paths).collect();

        for path in paths {
            self.apply_path(&path)?;
        }

        println!("{}", self.collector.as_table());
        Ok(())
    }

    fn apply_path(&mut self, path: &Path) -> crate::Result<()> {
        let filename = path.file_name().and_then(|name| name.to_str());
        let text = fs::read_to_string(path)?;
        self.collector.apply_str(filename, &text)
    }
}

fn extract_paths(path: &str) -> Box<dyn Iterator<Item = PathBuf>> {
    match fs::metadata(path) {
        Ok(metadata) => literal_path(path, metadata),
        Err(_) => glob_pattern(path),
    }
}

fn literal_path(path: &str, metadata: fs::Metadata) -> Box<dyn Iterator<Item = PathBuf>> {
    if metadata.is_file() {
        return Box::new(iter::once(path.into()));
    }

    let paths = walkdir::WalkDir::new(path)
        .contents_first(true)
        .into_iter()
        .filter_entry(|entry| {
            entry
                .metadata()
                .map(|meta| meta.file_type().is_file())
                .unwrap_or_default()
        })
        .filter_map(|entry| entry.ok().map(|entry| entry.path().into()));

    Box::new(paths)
}

fn glob_pattern(path: &str) -> Box<dyn Iterator<Item = PathBuf>> {
    let paths = match glob::glob(path) {
        Ok(paths) => paths,
        Err(_) => return Box::new(iter::empty()),
    };

    let paths = paths.filter_map(|item| item.ok()).filter(|candidate| {
        candidate
            .metadata()
            .map(|meta| meta.file_type().is_file())
            .unwrap_or_default()
    });

    Box::new(paths)
}

#[cfg(test)]
mod tests {
    use crate::collector::{Collector, Stats};

    static TEXT: &str = include_str!("../resources/sample.md");

    #[test]
    fn stats_are_correct() {
        let mut collector = Collector::new();
        collector.apply_str(None, TEXT).unwrap();

        let Stats {
            word_count,
            paragraph_count,
            ..
        } = collector.overall_stats();

        assert_eq!(321, word_count, "{:?}", collector.overall_stats());
        assert_eq!(9, paragraph_count, "{:?}", collector.overall_stats());
    }
}
