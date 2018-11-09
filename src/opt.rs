use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    #[structopt(parse(from_os_str))]
    path: Option<PathBuf>,
}

impl Opt {
    pub fn from_args() -> Opt {
        StructOpt::from_args()
    }

    pub fn path(&self) -> &Path {
        match self.path {
            None => Path::new("."),
            Some(ref path) => path,
        }
    }
}
