pub mod collector;
pub mod error;

pub use collector::DocumentStats;
pub use error::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;
