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
}
