
use std::time::{Duration, Instant};

pub fn mono_ns() -> u64 {
    static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    EPOCH.elapsed().as_nanos() as u64
}

pub fn tick_period(tick_hz: f64) -> Duration {
    Duration::from_nanos((1e9 / tick_hz.max(1.0)) as u64)
}
