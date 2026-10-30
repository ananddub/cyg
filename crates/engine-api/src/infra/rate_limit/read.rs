use std::net::IpAddr;

use uuid::Uuid;

use super::bucket::{IpRateLimiter, MAX_BUCKETS, Store};

impl<K: Eq + std::hash::Hash + Clone> Store<K> {
    /// Seconds until `amount` tokens would fit; `0` when the caller can proceed
    /// now, and a table that is off always says so.
    pub(super) fn retry_after(&self, key: &K, amount: f64) -> u32 {
        if !self.enabled() {
            return 0;
        }
        if !self.buckets.contains_key(key) {
            return if self.buckets.len() >= MAX_BUCKETS {
                1
            } else {
                0
            };
        }
        let deficit = amount - self.tokens_now(key);
        if deficit <= 0.0 {
            return 0;
        }
        let secs = deficit / self.refill_per_sec;
        if !secs.is_finite() {
            return u32::MAX;
        }
        (secs.ceil() as u64).clamp(1, u64::from(u32::MAX)) as u32
    }

    pub(super) fn tokens_now(&self, key: &K) -> f64 {
        match self.buckets.get(key) {
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
}

impl IpRateLimiter {
    pub fn retry_after(&self, addr: IpAddr) -> u32 {
        self.ips.retry_after(&addr, 1.0)
    }

    pub fn tokens_now(&self, addr: IpAddr) -> f64 {
        self.ips.tokens_now(&addr)
    }

    pub fn retry_after_player(&self, uid: Uuid) -> u32 {
        self.players.retry_after(&uid, 1.0)
    }

    /// The per-source counters only: a status page reads these as the flood it
    /// is watching for, and account buckets are a separate story.
    pub fn counts(&self) -> (u64, u64) {
        (self.ips.allowed, self.ips.denied)
    }

    pub fn squeezed(&self) -> u64 {
        self.ips.squeezed
    }

    pub fn swept(&self) -> u64 {
        self.ips.swept
    }

    pub fn tracked(&self) -> usize {
        self.ips.buckets.len()
    }

    pub fn player_counts(&self) -> (u64, u64) {
        (self.players.allowed, self.players.denied)
    }

    pub fn tracked_players(&self) -> usize {
        self.players.buckets.len()
    }
}
