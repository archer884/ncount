use std::path::PathBuf;

use crate::{collector::Collector, opt::Opts};

pub struct Application {
    options: Opts,
    collector: Collector,
}

impl Application {
    pub fn new(options: Opts) -> Self {
        Application {
            collector: Collector::new(),
            options,
        }
    }

    pub fn run(&mut self) -> crate::Result<()> {
        for path in read_paths(self.options.paths()) {
            self.collector.apply_path(&path)?;
        }

        println!("{}", self.collector.as_table(self.options.detail()));
        Ok(())
    }
}

fn read_paths<'a>(paths: impl Iterator<Item = &'a str> + 'a) -> impl Iterator<Item = PathBuf> + 'a {
    paths
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
}
