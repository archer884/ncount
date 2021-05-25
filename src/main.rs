mod app;
mod format;
mod opt;

use app::Application;
use opt::Opts;

fn main() -> ncount::Result<()> {
    Application::new(Opts::from_args()).run()
}
