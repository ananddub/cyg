use super::*;
use engine_bus::{Bus, SessionTelemetry, TICK_HISTOGRAM_BUCKETS};
use std::time::{Duration, Instant};

mod publish;
mod recorder;
mod sessions;

pub(super) fn publish_report() -> PublishReport {
    PublishReport { tick_hz_est: 128.0, tick_rate_hz: 128, players: 3, session_cap: 8, ..Default::default() }
}
