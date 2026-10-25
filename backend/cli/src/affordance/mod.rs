pub mod conversation;
pub mod parser;

pub use parser::{AffordanceRegistry, CommandAffordance};

#[cfg(test)]
#[path = "affordance_tests.rs"]
mod tests;
