mod app;
mod collector;
mod error;
mod opt;

use app::Application;
use opt::Args;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn run(opts: Args) -> Result<()> {
    Application::new(opts).run()
}
