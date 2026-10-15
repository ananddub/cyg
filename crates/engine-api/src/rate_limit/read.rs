use std::net::IpAddr;

use super::bucket::{IpRateLimiter, MAX_BUCKETS};

impl IpRateLimiter {
    /// Whole seconds this source must wait for its next token — the value put
    /// in `Retry-After`.
    ///
    /// Derived from the bucket, not from a constant: the deficit divided by the
    /// refill rate, rounded *up* so a client that obeys it is never refused
    /// again. `0` means this source can proceed right now, which keeps a stale
    /// `Retry-After` from sticking to a healthy address. Refill is applied as of
    /// this read, so a caller that waits the advertised time sees a smaller
    /// number, never a larger one.
    pub fn retry_after(&self, addr: IpAddr) -> u32 {
        if !self.buckets.contains_key(&addr) {
            // Never seen: a full burst, unless the ceiling is full, in which
            // case `allow` would refuse it — so do not advertise "go now".
            return if self.buckets.len() >= MAX_BUCKETS { 1 } else { 0 };
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

    /// Tokens this source effectively holds right now, after lazy refill.
    /// The deny/allow decision itself uses the mutable path in `allow`.
    pub fn tokens_now(&self, addr: IpAddr) -> f64 {
        match self.buckets.get(&addr) {
            // A source we have never seen has a full burst waiting.
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

    /// Requests refused by the tracked-source ceiling rather than by an empty
    /// bucket. A growing number here means address churn, not a slow client.
    pub fn squeezed(&self) -> u64 {
        self.squeezed
    }

    /// Total buckets examined by the ceiling's reclaim path.
    pub fn swept(&self) -> u64 {
        self.swept
    }

    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }
}
