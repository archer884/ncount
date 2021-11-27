mod app;
mod collector;
mod error;
mod opt;

use app::Application;
use opt::Opts;

type Result<T, E = error::Error> = std::result::Result<T, E>;

fn main() {
    let opts = Opts::from_args();
    if let Err(e) = run(opts) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn run(opts: Opts) -> Result<()> {
    Application::new(opts).run()
}
