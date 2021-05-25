mod app;
mod opt;

use app::Application;
use opt::Opts;

fn main() -> ncount::Result<()> {
    Application::new(Opts::from_args()).run()
}
