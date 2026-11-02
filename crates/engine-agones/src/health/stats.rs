use std::time::Duration;

use serde::Serialize;

use super::UNHEALTHY_AFTER_MISSES;
use crate::error::SidecarError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingStats {
    pub pings: u64,
    pub successes: u64,
    pub failures: u64,
    pub consecutive_failures: u32,
    pub max_consecutive_failures: u32,
    pub unreachable_failures: u32,
    pub timeout_failures: u32,
    pub rejected_failures: u32,
    pub malformed_failures: u32,
    pub unhealthy_declared: bool,
    pub last_latency_ms: u64,
    pub total_latency_ms: u64,
}

impl PingStats {
    pub(super) fn note_success(&mut self, latency: Duration) {
        self.pings += 1;
        self.successes += 1;
        self.consecutive_failures = 0;
        self.record_latency(latency);
    }

    pub(super) fn note_failure(&mut self, latency: Duration, error: &SidecarError) {
        self.pings += 1;
        self.failures += 1;
        self.consecutive_failures += 1;
        self.max_consecutive_failures =
            self.max_consecutive_failures.max(self.consecutive_failures);
        if self.consecutive_failures >= UNHEALTHY_AFTER_MISSES {
            self.unhealthy_declared = true;
        }
        match error.kind().as_str() {
            "unreachable" => self.unreachable_failures += 1,
            "timeout" => self.timeout_failures += 1,
            "rejected" => self.rejected_failures += 1,
            _ => self.malformed_failures += 1,
        }
        self.record_latency(latency);
    }

    fn record_latency(&mut self, latency: Duration) {
        self.last_latency_ms = latency.as_millis() as u64;
        self.total_latency_ms = self
            .total_latency_ms
            .saturating_add(latency.as_millis() as u64);
    }

    pub fn mean_latency_ms(&self) -> u64 {
        self.total_latency_ms.checked_div(self.pings).unwrap_or(0)
    }
}
