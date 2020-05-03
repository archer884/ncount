use crate::{collector::Collector, opt::Opt};
use std::path::{Path, PathBuf};
use std::{fs, io, iter};

pub struct Application {
    options: Opt,
    collector: Collector,
}

impl Application {
    pub fn new(options: Opt) -> Self {
        Application {
            collector: Collector::new(),
            options,
        }
    }

    pub fn run(&mut self) -> crate::Result<()> {
        let paths: Vec<_> = self.options.paths().flat_map(extract_paths).collect();

        for path in paths {
            self.apply_path(&path)?;
        }

        println!("{}", self.collector.as_table(self.options.detail()));
        Ok(())
    }

    fn apply_path(&mut self, path: &Path) -> crate::Result<()> {
        let filename = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Cannot apply directory path"))?
            .to_string_lossy();

        self.collector
            .apply_str(&*filename, &fs::read_to_string(path)?);
        Ok(())
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
