pub mod merge;
pub mod split;
pub mod types;

pub use merge::*;
pub use split::*;
pub use types::*;

#[cfg(test)]
#[path = "triad_tests.rs"]
mod tests;
