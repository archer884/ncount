use std::env;

pub fn init() {
    let environment = env::var("RUST_LOG").or_else(|_| env::var("LOG"));
    if let Ok(filter) = environment {
        tracing_subscriber::FmtSubscriber::builder()
            .pretty()
            .without_time()
            .with_env_filter(filter)
            .init();
    }
}
