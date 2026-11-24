use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    Allow,

    Disconnect,
}

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

    pub fn with_window(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window,
            window,
            window_start: Instant::now(),
            count: 0,
        }
    }

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

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn resets_in(&self, now: Instant) -> Duration {
        self.window
            .saturating_sub(now.duration_since(self.window_start))
    }
}

#[derive(Debug, Default)]
pub struct MessageRateLimits {
    max_per_window: u32,
    window: Duration,
    guards: HashMap<String, MessageRateGuard>,

    last_seen: HashMap<String, Instant>,

    last_sweep: Option<Instant>,
}

const IDLE_EVICT: Duration = Duration::from_secs(30);

const EVICT_INTERVAL: Duration = Duration::from_secs(5);

impl MessageRateLimits {
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
            self.guards.remove(session);
            self.last_seen.remove(session);
        }
        decision
    }

    pub fn remove(&mut self, session: &str) {
        self.guards.remove(session);
        self.last_seen.remove(session);
    }

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

    pub fn len(&self) -> usize {
        self.guards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.guards.is_empty()
    }
}

#[cfg(test)]
mod tests;
