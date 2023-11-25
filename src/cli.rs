use std::{
    fs, iter,
    path::{Path, PathBuf},
};

use clap::Parser;
use either::Either;
use globwalk::GlobWalker;

#[derive(Debug, Parser)]
pub struct Args {
    /// file or directory paths
    paths: Vec<String>,

    /// print extended information
    #[arg(short, long)]
    verbose: bool,
}

impl Args {
    pub fn parse() -> Self {
        Parser::parse()
    }

    pub fn materialize_files(&self) -> Vec<PathBuf> {
        // We still have to sort these things because the default enumeration order
        // on non-Windows file systems is freaking inode order. Thanks, guys!
        let mut files: Vec<_> = self.files().collect();
        files.sort();
        files
    }

    fn files<'a>(&'a self) -> impl Iterator<Item = PathBuf> + 'a {
        let sources = self.paths.iter().filter_map(|candidate| {
            if Path::new(candidate).exists() {
                Some(Either::Left(candidate))
            } else {
                globwalk::glob_builder(candidate)
                    .max_depth(1)
                    .build()
                    .ok()
                    .map(Either::Right)
            }
        });

        sources.flat_map(|source| match source {
            Either::Left(path) => Either::Left(iter_path_files(path)),
            Either::Right(glob) => Either::Right(iter_glob_files(glob)),
        })
    }
}

fn iter_path_files(path: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    let path = path.as_ref();
    if path.is_file() {
        Either::Left(iter::once(path.into()))
    } else {
        let paths = fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                entry
                    .file_type()
                    .ok()
                    .map(|kind| (kind, entry))
                    .filter(|x| x.0.is_file())
                    .map(|(_, entry)| entry.path())
            });
        Either::Right(paths)
    }
}

fn iter_glob_files(glob: GlobWalker) -> impl Iterator<Item = PathBuf> {
    glob.filter_map(|entry| {
        entry
            .ok()
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
    })
}
