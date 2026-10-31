//! # Per-Client Message Rate Limiting (DoS Shield)
//!
//! The Colyseus `maxMessagesPerSecond` contract: every message a client posts
//! counts against a fixed one-second window; a client that exceeds the budget
//! is force-disconnected, not throttled — a hacked client or broken bot loop
//! must lose its seat, not hold it.
//!
//! Deliberately distinct from [`engine-server`]'s token-bucket *input* limiter
//! (which drops surplus samples instead of disconnecting): input flooding
//! degrades gracefully, message flooding is abuse.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The decision for one inbound message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    /// Under budget — process the message.
    Allow,
    /// Window budget exhausted — the client must be force-disconnected and
    /// this message dropped.
    Disconnect,
}

/// One client's fixed-window message counter.
///
/// A fixed window (not a token bucket) matches the contract exactly: N
/// messages per second, reset every second of server time. O(1) per message,
/// zero allocation, no drift — the window anchors to the first message that
/// starts it, and the counter never accumulates credit.
#[derive(Debug, Clone)]
pub struct MessageRateGuard {
    max_per_window: u32,
    window: Duration,
    window_start: Instant,
    count: u32,
}

impl MessageRateGuard {
    pub fn new(max_per_second: u32) -> Self {
        Self::with_window(max_per_second, Duration::from_millis(1000))
    }

    /// `max_per_window` messages per `window` of server time.
    pub fn with_window(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window,
            window,
            window_start: Instant::now(),
            count: 0,
        }
    }

    /// Count one message against the window.
    #[inline]
    pub fn allow(&mut self, now: Instant) -> RateDecision {
        if now.duration_since(self.window_start) >= self.window {
            self.window_start = now;
            self.count = 0;
        }
        self.count += 1;
        if self.count > self.max_per_window {
            RateDecision::Disconnect
        } else {
            RateDecision::Allow
        }
    }

    /// Messages counted in the current window (telemetry).
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Time until the current window resets (telemetry / tests).
    pub fn resets_in(&self, now: Instant) -> Duration {
        self.window
            .saturating_sub(now.duration_since(self.window_start))
    }
}

/// Per-session guard table with idle eviction, so departed sessions do not
/// pin entries forever.
#[derive(Debug, Default)]
pub struct MessageRateLimits {
    max_per_window: u32,
    window: Duration,
    guards: HashMap<String, MessageRateGuard>,
    /// Sessions most recently allowed — eviction candidates.
    last_seen: HashMap<String, Instant>,
    /// The last eviction sweep (gated, see [`Self::sweep`]).
    last_sweep: Option<Instant>,
}

/// How long an idle session's guard is retained before eviction sweeps.
const IDLE_EVICT: Duration = Duration::from_secs(30);
/// Eviction runs at most once per this interval, not per message.
const EVICT_INTERVAL: Duration = Duration::from_secs(5);

impl MessageRateLimits {
    /// `max_per_second == 0` disables limiting entirely: [`Self::check`]
    /// always allows and no state is kept.
    pub fn new(max_per_second: u32) -> Self {
        Self {
            max_per_window: max_per_second,
            window: Duration::from_millis(1000),
            guards: HashMap::new(),
            last_seen: HashMap::new(),
            last_sweep: None,
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.max_per_window > 0
    }

    /// Count one message from `session`.
    #[inline]
    pub fn check(&mut self, session: &str, now: Instant) -> RateDecision {
        if !self.is_enabled() {
            return RateDecision::Allow;
        }
        self.last_seen.insert(session.to_string(), now);
        let max = self.max_per_window;
        let window = self.window;
        let decision = self
            .guards
            .entry(session.to_string())
            .or_insert_with(|| MessageRateGuard::with_window(max, window))
            .allow(now);
        if decision == RateDecision::Disconnect {
            // The violator is going away — drop its state now instead of
            // waiting for the idle sweep.
            self.guards.remove(session);
            self.last_seen.remove(session);
        }
        decision
    }

    /// Forget one session (it left or was disconnected).
    pub fn remove(&mut self, session: &str) {
        self.guards.remove(session);
        self.last_seen.remove(session);
    }

    /// Drop guards idle past the retention window. Cheap: swept at most once
    /// per [`EVICT_INTERVAL`] regardless of call rate.
    pub fn sweep(&mut self, now: Instant) {
        if !self.is_enabled() || self.guards.is_empty() {
            return;
        }
        if self
            .last_sweep
            .is_some_and(|at| now.duration_since(at) < EVICT_INTERVAL)
        {
            return;
        }
        self.last_sweep = Some(now);
        let idle: Vec<String> = self
            .last_seen
            .iter()
            .filter(|(_, at)| now.duration_since(**at) > IDLE_EVICT)
            .map(|(s, _)| s.clone())
            .collect();
        for s in idle {
            self.guards.remove(&s);
            self.last_seen.remove(&s);
        }
    }

    /// Number of tracked sessions (telemetry).
    pub fn len(&self) -> usize {
        self.guards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.guards.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_window_counts_and_disconnects() {
        let mut guard = MessageRateGuard::new(3);
        let t0 = Instant::now();
        assert_eq!(guard.allow(t0), RateDecision::Allow);
        assert_eq!(guard.allow(t0), RateDecision::Allow);
        assert_eq!(guard.allow(t0), RateDecision::Allow);
        assert_eq!(guard.allow(t0), RateDecision::Disconnect);
        assert_eq!(guard.allow(t0), RateDecision::Disconnect);
    }

    #[test]
    fn window_reset_restores_budget_without_credit_carry() {
        let mut guard = MessageRateGuard::new(2);
        let t0 = Instant::now();
        assert_eq!(guard.allow(t0), RateDecision::Allow);
        assert_eq!(guard.allow(t0), RateDecision::Allow);
        assert_eq!(guard.allow(t0), RateDecision::Disconnect);
        // One second later the window is fresh: exactly 2 more, then done.
        let t1 = t0 + Duration::from_millis(1001);
        assert_eq!(guard.allow(t1), RateDecision::Allow);
        assert_eq!(guard.allow(t1), RateDecision::Allow);
        assert_eq!(guard.allow(t1), RateDecision::Disconnect);
    }

    #[test]
    fn zero_limit_disables_everything() {
        let mut limits = MessageRateLimits::new(0);
        let t0 = Instant::now();
        for _ in 0..10_000 {
            assert_eq!(limits.check("spammer", t0), RateDecision::Allow);
        }
        assert!(limits.is_empty(), "disabled limiter keeps no state");
    }

    #[test]
    fn exceeded_session_is_dropped_immediately() {
        let mut limits = MessageRateLimits::new(2);
        let t0 = Instant::now();
        assert_eq!(limits.check("a", t0), RateDecision::Allow);
        assert_eq!(limits.check("a", t0), RateDecision::Allow);
        assert_eq!(limits.check("a", t0), RateDecision::Disconnect);
        assert!(limits.is_empty(), "violator state is dropped with the seat");
    }

    #[test]
    fn sessions_are_independent() {
        let mut limits = MessageRateLimits::new(2);
        let t0 = Instant::now();
        assert_eq!(limits.check("a", t0), RateDecision::Allow);
        assert_eq!(limits.check("b", t0), RateDecision::Allow);
        assert_eq!(limits.check("a", t0), RateDecision::Allow);
        assert_eq!(limits.check("a", t0), RateDecision::Disconnect);
        assert_eq!(limits.check("b", t0), RateDecision::Allow);
        assert_eq!(limits.check("b", t0), RateDecision::Disconnect);
    }

    #[test]
    fn sweep_drops_only_idle_sessions() {
        let mut limits = MessageRateLimits::new(100);
        let t0 = Instant::now();
        limits.check("active", t0);
        limits.check("gone", t0);
        // Sweep is interval-gated: an immediate sweep is a no-op.
        limits.sweep(t0);
        assert_eq!(limits.len(), 2);
        // Active keeps posting; gone goes quiet. Eviction needs both the
        // 30s idle threshold and the 5s sweep gate to pass (~35s).
        let mut t = t0;
        for _ in 0..36 {
            t += Duration::from_secs(1);
            limits.check("active", t);
            limits.sweep(t);
        }
        assert_eq!(limits.len(), 1, "only the active session remains");
        limits.remove("active");
        assert!(limits.is_empty());
    }
}
