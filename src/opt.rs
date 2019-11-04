use std::iter;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    paths: Vec<String>,
}

impl Opt {
    pub fn from_args() -> Opt {
        StructOpt::from_args()
    }

    pub fn paths<'a>(&'a self) -> Box<dyn Iterator<Item = &str> + 'a> {
        if self.paths.is_empty() {
            Box::new(iter::once("."))
        } else {
            Box::new(self.paths.iter().map(AsRef::as_ref))
        }
    }
}
