use app::Application;
use opt::Opts;

mod app;
mod collector;
mod error;
mod opt;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() -> Result<()> {
    Application::new(Opts::from_args()).run()
}
