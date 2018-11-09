mod app;
mod collector;
mod error;
mod opt;
mod parse;

use crate::{app::Application, opt::Opt};

fn main() -> error::Result<()> {
    Application.run(&Opt::from_args())
}
