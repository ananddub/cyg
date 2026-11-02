pub const UNHEALTHY_AFTER_MISSES: u32 = 3;

mod beat;
mod outcome;
mod stats;

#[cfg(test)]
mod tests;

pub use beat::{Healthbeat, next_due_after};
pub use outcome::PingOutcome;
pub use stats::PingStats;
