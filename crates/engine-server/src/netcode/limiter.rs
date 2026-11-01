use std::time::Instant;

#[derive(Clone, Debug)]
pub struct InputRateLimiter {
    tokens: f64,
    rate_per_sec: f64,
    burst: f64,
    last: Instant,
}

impl InputRateLimiter {
    pub const SAMPLES_PER_TICK: u32 = 8;

    pub fn for_tick_rate(tick_rate: u32) -> Self {
        let rate = tick_rate.saturating_mul(Self::SAMPLES_PER_TICK).max(64);
        Self::new(rate, (rate / 4).max(8) as f64)
    }

    pub fn new(rate_per_sec: u32, burst: f64) -> Self {
        let rate = rate_per_sec.max(1) as f64;
        let burst = burst.max(1.0);
        Self {
            tokens: burst,
            rate_per_sec: rate,
            burst,
            last: Instant::now(),
        }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate_per_sec).min(self.burst);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Default for InputRateLimiter {
    fn default() -> Self {
        Self::new(256, 32.0)
    }
}
