use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),

    #[error(transparent)]
    Notify(#[from] notify::Error),

    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("no input: pass files, or pipe text on stdin")]
    NoInput,

    #[cfg(feature = "progress")]
    #[error("progress needs files: pass the paths or quoted globs to chart")]
    NoPaths,

    #[cfg(feature = "progress")]
    #[error("not inside a git repository: {0}")]
    NotARepository(PathBuf),

    #[cfg(feature = "progress")]
    #[error("{0} is outside the git repository at {1}")]
    OutsideRepository(PathBuf, PathBuf),

    #[cfg(feature = "progress")]
    #[error("invalid glob pattern {0:?}: {1}")]
    BadPattern(String, globset::Error),

    #[cfg(feature = "progress")]
    #[error("no files matched, in the working tree or the charted history")]
    NothingMatched,

    #[cfg(feature = "progress")]
    #[error("reading git history: {0}")]
    Git(Box<dyn std::error::Error + Send + Sync>),
}
