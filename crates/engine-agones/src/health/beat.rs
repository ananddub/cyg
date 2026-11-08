use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::UNHEALTHY_AFTER_MISSES;
use super::outcome::PingOutcome;
use super::stats::PingStats;
use crate::client::AgonesClient;
use crate::error::SidecarError;
use crate::state::AgonesController;

const SLEEP_SLICE: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct Healthbeat {
    client: AgonesClient,
    interval: Duration,
    max_backoff: Duration,
    pub(super) stats: PingStats,
    next_due: Instant,
    last_error: Option<SidecarError>,
}

impl Healthbeat {
    pub fn new(client: AgonesClient, interval: Duration) -> Healthbeat {
        let interval = interval.max(Duration::from_millis(1));
        Healthbeat {
            client,
            interval,
            max_backoff: interval.saturating_mul(8),
            stats: PingStats::default(),
            next_due: Instant::now(),
            last_error: None,
        }
    }

    pub fn with_max_backoff(mut self, max_backoff: Duration) -> Healthbeat {
        self.max_backoff = max_backoff.max(self.interval);
        self
    }

    pub fn client(&self) -> &AgonesClient {
        &self.client
    }

    pub fn into_client(self) -> AgonesClient {
        self.client
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn max_backoff(&self) -> Duration {
        self.max_backoff
    }

    pub fn stats(&self) -> PingStats {
        self.stats
    }

    pub fn next_due(&self) -> Instant {
        self.next_due
    }

    pub fn last_error(&self) -> Option<&SidecarError> {
        self.last_error.as_ref()
    }

    pub fn current_delay(&self) -> Duration {
        let streak = self.stats.consecutive_failures;
        let shift = streak.saturating_sub(1).min(3);
        self.interval
            .saturating_mul(1u32 << shift)
            .min(self.max_backoff)
    }

    pub fn wait(&self, now: Instant) -> Duration {
        self.next_due.saturating_duration_since(now)
    }

    pub fn is_due(&self, now: Instant) -> bool {
        now >= self.next_due
    }

    pub fn reschedule(&mut self, now: Instant) {
        self.next_due = saturating_add(now, self.current_delay());
    }

    pub fn ping(&mut self, now: Instant) -> PingOutcome {
        let started = Instant::now();
        let result = self.client.send_health_ping();
        let latency = started.elapsed();

        let before_streak = self.stats.consecutive_failures;
        let error = match result {
            Ok(()) => {
                self.stats.note_success(latency);
                self.last_error = None;
                None
            }
            Err(e) => {
                self.stats.note_failure(latency, &e);
                self.last_error = Some(e.clone());
                Some(e)
            }
        };

        let consecutive_failures = self.stats.consecutive_failures;
        self.next_due = next_due_after(self.next_due, now, self.current_delay());

        PingOutcome {
            ok: error.is_none(),
            latency,
            consecutive_failures,
            just_missed_the_window: error.is_some()
                && consecutive_failures == UNHEALTHY_AFTER_MISSES
                && before_streak < UNHEALTHY_AFTER_MISSES,
            error,
        }
    }

    pub fn run_until_stop(&mut self, stop: &AtomicBool, mut on_outcome: impl FnMut(&PingOutcome)) {
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if self.is_due(now) {
                let outcome = self.ping(now);
                on_outcome(&outcome);
            }
            let remaining = self.wait(Instant::now());
            if remaining.is_zero() {
                continue;
            }
            std::thread::sleep(remaining.min(SLEEP_SLICE));
        }
    }
}

pub fn next_due_after(prev_due: Instant, now: Instant, delay: Duration) -> Instant {
    let scheduled = saturating_add(prev_due, delay);
    if scheduled > now {
        scheduled
    } else {
        saturating_add(now, delay)
    }
}

fn saturating_add(at: Instant, delta: Duration) -> Instant {
    at.checked_add(delta).unwrap_or(at)
}
