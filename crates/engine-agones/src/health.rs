//! The health beat: the loop that decides *when* to ping, how hard to back off,
//! and what to report about it.
//!
//! Two properties matter more than anything else here, because Agones deletes a
//! pod that misses three intervals:
//!
//! 1. **The loop never stops.** A refused connection, a timeout, a garbage reply
//!    is a data point, not a fatal error. The only thing that ends the loop is an
//!    explicit stop flag.
//! 2. **The interval does not drift.** The next ping is scheduled from the
//!    *previous scheduled time*, not from when the last one finished, so a
//!    30 ms sidecar hiccup does not push every later ping 30 ms late.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::client::AgonesClient;
use crate::error::SidecarError;
use crate::state::AgonesController;

/// Misses in a row after which the agent has marked this pod Unhealthy. Agones
/// counts three, so a client that logs on the tenth is a client that logs too late.
pub const UNHEALTHY_AFTER_MISSES: u32 = 3;

/// Longest single sleep in the loop. Bounds how long `stop` takes to be noticed
/// without spinning the thread.
const SLEEP_SLICE: Duration = Duration::from_millis(20);

/// Ping counters, exactly as they are reported to `/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingStats {
    /// Heartbeats attempted.
    pub pings: u64,
    /// Heartbeats that got a 2xx.
    pub successes: u64,
    /// Heartbeats that did not.
    pub failures: u64,
    /// Current streak of failures; resets on the first success.
    pub consecutive_failures: u32,
    /// Longest streak ever seen — the number that explains a restart.
    pub max_consecutive_failures: u32,
    /// Round trips that could not open a socket at all: no sidecar, wrong port.
    pub unreachable_failures: u32,
    /// Round trips that got no complete reply before the deadline: a sick node.
    pub timeout_failures: u32,
    /// Round trips the sidecar answered 4xx/5xx to: a state disagreement.
    pub rejected_failures: u32,
    /// Round trips whose reply we could not parse.
    pub malformed_failures: u32,
    /// True once the streak reached [`UNHEALTHY_AFTER_MISSES`]. Stays true: a
    /// pod that was declared unhealthy is a fact about this process's life.
    pub unhealthy_declared: bool,
    /// Last round-trip time, for "is the sidecar slow or is it gone".
    pub last_latency_ms: u64,
    /// Sum of round-trip times; divide by `pings` for a mean.
    pub total_latency_ms: u64,
}

impl PingStats {
    fn note_success(&mut self, latency: Duration) {
        self.pings += 1;
        self.successes += 1;
        self.consecutive_failures = 0;
        self.record_latency(latency);
    }

    fn note_failure(&mut self, latency: Duration, error: &SidecarError) {
        self.pings += 1;
        self.failures += 1;
        self.consecutive_failures += 1;
        self.max_consecutive_failures = self.max_consecutive_failures.max(self.consecutive_failures);
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
        self.total_latency_ms = self.total_latency_ms.saturating_add(latency.as_millis() as u64);
    }

    /// Mean round-trip time in milliseconds, `0` before the first ping.
    pub fn mean_latency_ms(&self) -> u64 {
        self.total_latency_ms.checked_div(self.pings).unwrap_or(0)
    }
}

/// The result of one heartbeat, handed to whoever wants to log or record it.
#[derive(Debug, Clone)]
pub struct PingOutcome {
    /// Did the sidecar accept the ping.
    pub ok: bool,
    pub latency: Duration,
    /// Failure streak *after* this ping, so a log line can say "3 in a row".
    pub consecutive_failures: u32,
    /// The failure, when there was one.
    pub error: Option<SidecarError>,
    /// True on the ping whose streak first reached
    /// [`UNHEALTHY_AFTER_MISSES`] — the moment worth a loud log, logged once
    /// per streak rather than every interval.
    pub just_missed_the_window: bool,
}

impl PingOutcome {
    pub fn error_kind(&self) -> Option<&'static str> {
        self.error.as_ref().map(|e| e.kind().as_str())
    }

    /// A synthetic outcome for a beat we could not reach at all (its lock was
    /// poisoned). Reported as a failure, but it never touched the socket and
    /// therefore never touched the counters either.
    pub(crate) fn unavailable() -> PingOutcome {
        PingOutcome {
            ok: false,
            latency: Duration::ZERO,
            consecutive_failures: 0,
            error: Some(SidecarError::Malformed {
                reason: "the health beat's own lock was poisoned".into(),
            }),
            just_missed_the_window: false,
        }
    }
}

/// Owns the cadence: the interval, the backoff, the counters and the schedule.
#[derive(Debug)]
pub struct Healthbeat {
    client: AgonesClient,
    interval: Duration,
    max_backoff: Duration,
    stats: PingStats,
    next_due: Instant,
    last_error: Option<SidecarError>,
}

impl Healthbeat {
    /// `interval` is the configured `health_interval_ms`. The backoff ceiling is
    /// eight intervals — far enough past the unhealthy window that a node which
    /// is merely busy gets no extra traffic, near enough that a recovered
    /// sidecar is noticed within seconds.
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

    /// How long to wait before the next ping given the current failure streak:
    /// the plain interval while healthy, then doubling (×2, ×4, ×8) capped at
    /// `max_backoff`. A connection refused in microseconds does not need a backoff
    /// that long to recover from — but a kubelet under load does.
    pub fn current_delay(&self) -> Duration {
        let streak = self.stats.consecutive_failures;
        let shift = streak.saturating_sub(1).min(3);
        self.interval.saturating_mul(1u32 << shift).min(self.max_backoff)
    }

    /// Time until the next ping is due.
    pub fn wait(&self, now: Instant) -> Duration {
        self.next_due.saturating_duration_since(now)
    }

    pub fn is_due(&self, now: Instant) -> bool {
        now >= self.next_due
    }

    /// Move the schedule to `now + current_delay()` without pinging — used when a
    /// transition elsewhere (allocation, shutdown) has just told the sidecar
    /// something and an immediate heartbeat would be noise.
    pub fn reschedule(&mut self, now: Instant) {
        self.next_due = saturating_add(now, self.current_delay());
    }

    /// One heartbeat, with its counters updated and the next one scheduled.
    /// `now` is the scheduled tick, so the loop that called this stays drift-free.
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

    /// The blocking loop, on a dedicated OS thread. Pings when due, sleeps in
    /// short slices otherwise, and returns as soon as `stop` is set. Never
    /// panics on a sidecar failure and never returns an error: the pod's fate is
    /// the agent's decision, our job is to keep knocking.
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

/// Where the next ping is due, given the one that just happened.
///
/// Scheduling from `prev_due` (not from `now`) is what absorbs request latency:
/// with a 40 ms `delay` and a sidecar that answers in 15 ms, the pings stay at
/// `t0, t0+40, t0+80` instead of sliding 15 ms later every time. Only when a
/// request overruns the whole window does the schedule restart from completion —
/// which also makes a burst impossible, since the returned instant is always in
/// the future.
pub fn next_due_after(prev_due: Instant, now: Instant, delay: Duration) -> Instant {
    let scheduled = saturating_add(prev_due, delay);
    if scheduled > now { scheduled } else { saturating_add(now, delay) }
}

fn saturating_add(at: Instant, delta: Duration) -> Instant {
    // `Instant + Duration` panics on overflow; an instant 500 years out is
    // unreachable in practice, but a lifecycle loop must not be the thing that
    // finally proves me wrong.
    at.checked_add(delta).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_with_streak(streak: u32) -> Healthbeat {
        let mut hb = Healthbeat::new(
            AgonesClient::new(crate::client::SidecarConfig::loopback(1)),
            Duration::from_millis(40),
        );
        hb.stats.consecutive_failures = streak;
        hb.stats.pings = u64::from(streak);
        hb
    }

    #[test]
    fn backoff_doubles_then_caps_at_max_backoff() {
        let i = || Duration::from_millis(40);
        assert_eq!(stats_with_streak(0).current_delay(), i());
        assert_eq!(stats_with_streak(1).current_delay(), i());
        assert_eq!(stats_with_streak(2).current_delay(), Duration::from_millis(80));
        assert_eq!(stats_with_streak(3).current_delay(), Duration::from_millis(160));
        assert_eq!(stats_with_streak(4).current_delay(), Duration::from_millis(320));
        // 8 × interval is the ceiling, whatever the streak.
        assert_eq!(stats_with_streak(40).current_delay(), Duration::from_millis(320));
        let capped = stats_with_streak(9).with_max_backoff(Duration::from_millis(100));
        assert_eq!(capped.current_delay(), Duration::from_millis(100));
    }

    #[test]
    fn schedule_absorbs_latency_without_drift() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(50);
        // The ping finished 10 ms after it was due; the next one stays on the
        // original grid rather than sliding 10 ms later every time.
        let next = next_due_after(t0, t0 + Duration::from_millis(10), interval);
        assert_eq!(next, t0 + interval);
        // …and it keeps doing so across a full window's worth of pings.
        let after = next_due_after(next, next + Duration::from_millis(10), interval);
        assert_eq!(after, t0 + interval * 2);
    }

    #[test]
    fn schedule_resyncs_when_a_ping_overruns_and_never_queues_a_burst() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(50);
        // A 120 ms stall is past the window: restart from completion.
        let next = next_due_after(t0, t0 + Duration::from_millis(120), interval);
        assert_eq!(next, t0 + Duration::from_millis(170));
        assert!(next > t0 + Duration::from_millis(120), "the next ping must be in the future");
    }

    #[test]
    fn a_backed_off_streak_delays_the_next_ping_too() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(50);
        let next = next_due_after(t0, t0 + Duration::from_millis(5), interval * 4);
        assert_eq!(next, t0 + interval * 4);
    }

    #[test]
    fn stats_count_by_error_kind_and_declare_unhealthy_once() {
        let mut s = PingStats::default();
        let boom = |kind: &str| SidecarError::Malformed { reason: kind.into() };
        s.note_failure(Duration::from_millis(1), &boom("a"));
        s.note_failure(Duration::from_millis(3), &boom("b"));
        assert!(!s.unhealthy_declared);
        assert_eq!(s.consecutive_failures, 2);
        s.note_failure(Duration::from_millis(2), &SidecarError::Timeout {
            addr: "127.0.0.1:9358".into(),
            after_ms: 1000,
        });
        assert!(s.unhealthy_declared, "three misses is the Agones window");
        assert_eq!(s.timeout_failures, 1);
        assert_eq!(s.malformed_failures, 2);
        assert_eq!(s.max_consecutive_failures, 3);
        s.note_success(Duration::from_millis(4));
        assert_eq!(s.consecutive_failures, 0, "a recovery resets the streak");
        assert!(s.unhealthy_declared, "but the declaration is history");
        assert_eq!(s.pings, 4);
        assert_eq!(s.mean_latency_ms(), (1 + 3 + 2 + 4) / 4);
    }
}
