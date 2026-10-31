use std::net::IpAddr;

use super::bucket::{IpRateLimiter, MAX_BUCKETS};

impl IpRateLimiter {
    pub fn retry_after(&self, addr: IpAddr) -> u32 {
        if !self.buckets.contains_key(&addr) {
            return if self.buckets.len() >= MAX_BUCKETS {
                1
            } else {
                0
            };
        }
        let deficit = 1.0 - self.tokens_now(addr);
        if deficit <= 0.0 {
            return 0;
        }
        let secs = deficit / self.refill_per_sec;
        if !secs.is_finite() {
            return u32::MAX;
        }
        (secs.ceil() as u64).clamp(1, u64::from(u32::MAX)) as u32
    }

    pub fn tokens_now(&self, addr: IpAddr) -> f64 {
        match self.buckets.get(&addr) {
            None => self.burst,
            Some(bucket) => {
                let elapsed = std::time::Instant::now()
                    .duration_since(bucket.last)
                    .as_secs_f64()
                    .max(0.0);
                (bucket.tokens + elapsed * self.refill_per_sec).min(self.burst)
            }
        }
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.allowed, self.denied)
    }

    pub fn squeezed(&self) -> u64 {
        self.squeezed
    }

    pub fn swept(&self) -> u64 {
        self.swept
    }

    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }
}
