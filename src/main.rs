mod app;
mod collector;
mod error;
mod opt;

pub type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() -> Result<()> {
    app::Application::with_opts(opt::Opt::from_args()).run()
}
