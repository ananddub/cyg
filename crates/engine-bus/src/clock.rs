//! Process-wide monotonic clock helpers shared by both planes.
//!
//! A snapshot's `updated_ms` and the reader's `age_ms` must come from the *same*
//! epoch, and the sim's `arrival_ns` and the recorder's `now_ns` likewise — so
//! this lives below both of them instead of being duplicated per crate.

use std::time::{Duration, Instant};

/// Monotonic nanoseconds since an arbitrary process-wide epoch.
pub fn mono_ns() -> u64 {
    static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    EPOCH.elapsed().as_nanos() as u64
}

/// Nominal tick period for `tick_hz`.
pub fn tick_period(tick_hz: f64) -> Duration {
    Duration::from_nanos((1e9 / tick_hz.max(1.0)) as u64)
}
