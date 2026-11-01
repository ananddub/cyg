use std::time::Instant;

use crate::plane::udp::plane::MAX_PACKETS_PER_SEC;

pub(super) struct TokenBucket {
    pub(super) tokens: f64,
    pub(super) last: Instant,
}

impl TokenBucket {
    pub(super) fn new_at(now: Instant) -> TokenBucket {
        TokenBucket {
            tokens: MAX_PACKETS_PER_SEC as f64,
            last: now,
        }
    }

    pub(super) fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens =
            (self.tokens + elapsed * MAX_PACKETS_PER_SEC as f64).min(MAX_PACKETS_PER_SEC as f64);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}
