use std::{
    fs, iter,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use either::Either;

use crate::error::Error;
use crate::Result;

#[derive(Debug, Parser)]
#[command(author, version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Launch an interactive TUI that watches the given files for changes
    Tui(CommonArgs),
}

#[derive(Debug, clap::Args)]
pub struct CommonArgs {
    /// file or directory paths
    paths: Vec<String>,

    /// filter output by heading
    #[arg(short, long)]
    filter: Option<String>,

    /// print extended information
    #[arg(short, long)]
    verbose: bool,
}

impl Args {
    pub fn parse() -> Self {
        Parser::parse()
    }
}

impl CommonArgs {
    pub fn materialize_files(&self) -> Result<Vec<PathBuf>> {
        // Resolve each input (file, dir, or glob) to a list of actual file
        // paths, then canonicalize so the TUI's watch path matches the
        // absolute paths notify hands back. We still have to sort because
        // the default enumeration order on non-Windows file systems is
        // freaking inode order. Thanks, guys!
        let mut files = Vec::new();
        for candidate in &self.paths {
            let p = Path::new(candidate);
            let entries: Vec<PathBuf> = if p.exists() {
                iter_path_files(p).collect()
            } else {
                globwalk::glob_builder(candidate)
                    .max_depth(1)
                    .build()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.into_path())
                    .collect()
            };
            if entries.is_empty() {
                return Err(Error::FileNotFound(p.to_path_buf()));
            }
            files.extend(entries);
        }
        files.sort();
        files = files
            .into_iter()
            .map(|p| fs::canonicalize(&p).map_err(|_| Error::FileNotFound(p)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    pub fn verbose(&self) -> bool {
        self.verbose
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `set_current_dir` is process-wide; serialize tests that touch it.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    fn args(paths: &[&str]) -> CommonArgs {
        CommonArgs {
            paths: paths.iter().map(|s| s.to_string()).collect(),
            filter: None,
            verbose: false,
        }
    }

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl CurrentDirGuard {
        fn enter(dir: &Path) -> Self {
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir).unwrap();
            Self { original }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn materialize_canonicalizes_relative_paths() {
        // The whole point: a relative path the user typed on the command line
        // must round-trip through to a canonical (absolute) form, because
        // that's what notify hands back as the event path and what the TUI's
        // `Watch::changed` then string-compares against.
        let _lock = CWD_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("foo.md");
        std::fs::write(&file, "# x\n\nhello world").unwrap();

        let _guard = CurrentDirGuard::enter(temp.path());
        let files = args(&["foo.md"]).materialize_files().unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].is_absolute(), "got relative: {:?}", files[0]);
        assert_eq!(files[0], file.canonicalize().unwrap());
    }

    #[test]
    fn materialize_reports_missing_file_with_path() {
        let err = args(&["definitely/does/not/exist.md"])
            .materialize_files()
            .unwrap_err();
        match err {
            Error::FileNotFound(p) => {
                assert_eq!(p, PathBuf::from("definitely/does/not/exist.md"));
            }
            other => panic!("expected FileNotFound, got {other:?}"),
        }
    }
}
