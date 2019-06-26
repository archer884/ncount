mod app;
mod collector;
mod error;
mod opt;
mod parse;

fn main() -> error::Result<()> {
    app::Application.run(&opt::Opt::from_args())
}
