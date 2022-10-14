use std::path::PathBuf;

use crate::{collector::DocumentStats, opt::Args};

pub struct Application {
    options: Args,
    document: DocumentStats,
}

impl Application {
    pub fn new(options: Args) -> Self {
        Application {
            document: DocumentStats::new(),
            options,
        }
    }

    pub fn run(&mut self) -> crate::Result<()> {
        for path in read_paths(self.options.paths()) {
            self.document.apply_path(&path)?;
        }

        if let Some(filter) = self.options.filter_by_heading() {
            self.document.filter_by_heading(filter);
        }

        println!("{}", self.document.as_table(self.options.verbose));
        Ok(())
    }
}

fn read_paths<'a>(paths: impl Iterator<Item = &'a str> + 'a) -> Vec<PathBuf> {
    let mut paths: Vec<_> = paths
        .into_iter()
        .filter_map(|path| normal_paths::extract_paths(path).ok())
        .flatten()
        .filter_map(|path| {
            let path = path.ok()?;
            if path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    // macOS likes to send files in non-lexical order
    paths.sort();
    paths
}
