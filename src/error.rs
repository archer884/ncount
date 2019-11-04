use crate::parse::Rule;
use std::io;
use std::result;

pub type Result<T, E = Error> = result::Result<T, E>;

type PestError = pest::error::Error<Rule>;

#[derive(Debug)]
pub enum Error {
    IO(io::Error),
    Parse(PestError),
    Pattern(glob::PatternError),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IO(e)
    }
}

impl From<PestError> for Error {
    fn from(e: PestError) -> Self {
        Error::Parse(e)
    }
}

impl From<walkdir::Error> for Error {
    fn from(e: walkdir::Error) -> Self {
        io::Error::from(e).into()
    }
}

impl From<glob::PatternError> for Error {
    fn from(e: glob::PatternError) -> Self {
        Error::Pattern(e)
    }
}
