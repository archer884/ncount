use crate::parse::Rule;
use std::io;
use std::result;

pub type Result<T> = result::Result<T, Error>;

type PestError = pest::error::Error<Rule>;

#[derive(Debug)]
pub enum Error {
    IO(io::Error),
    Parse(PestError),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::IO(e)
    }
}

impl From<PestError> for Error {
    fn from(e: PestError) -> Error {
        Error::Parse(e)
    }
}

impl From<walkdir::Error> for Error {
    fn from(e: walkdir::Error) -> Error {
        // Sloppy, but whatever...
        io::Error::from(e).into()
    }
}
