use std::{error, fmt, io};

#[derive(Debug)]
pub enum Error {
    IO(io::Error),
    Pattern(glob::PatternError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::IO(e) => e.fmt(f),
            Self::Pattern(e) => e.fmt(f),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::IO(e) => Some(e),
            Self::Pattern(e) => Some(e),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IO(e)
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
