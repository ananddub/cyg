//! Process-wide monotonic clock. Both planes must stamp and read from the
//! *same* epoch, or sample ages and latencies are meaningless.

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
