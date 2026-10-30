use std::net::IpAddr;

use uuid::Uuid;

use super::bucket::{Bucket, IpRateLimiter, MAKEROOM_SCAN_BUDGET, MAX_BUCKETS, Store};

impl<K: Eq + std::hash::Hash + Clone> Store<K> {
    /// Takes `amount` tokens for `key`, creating its bucket when it is new.
    pub(super) fn charge(&mut self, key: K, amount: f64) -> bool {
        if !self.enabled() {
            return true;
        }
        let now = std::time::Instant::now();
        let burst = self.burst;
        let refill = self.refill_per_sec;
        if let Some(bucket) = self.buckets.get_mut(&key) {
            if bucket.tokens < burst {
                let elapsed = now.duration_since(bucket.last).as_secs_f64();
                bucket.tokens = (bucket.tokens + elapsed * refill).min(burst);
                bucket.last = now;
            }
        } else {
            if self.buckets.len() >= MAX_BUCKETS && !self.make_room() {
                self.denied += 1;
                self.squeezed += 1;
                return false;
            }
            self.buckets.insert(
                key.clone(),
                Bucket {
                    tokens: burst,
                    last: now,
                },
            );
            self.order.push_back(key.clone());
        }
        let bucket = self.buckets.get_mut(&key).expect("bucket was just created");
        if bucket.tokens >= amount {
            bucket.tokens -= amount;
            self.allowed += 1;
            true
        } else {
            self.denied += 1;
            false
        }
    }

    /// Frees a slot by dropping a key that has already refilled, sweeping at
    /// most `MAKEROOM_SCAN_BUDGET` entries so one call cannot starve the loop.
    fn make_room(&mut self) -> bool {
        let burst = self.burst;
        let refill = self.refill_per_sec;
        let now = std::time::Instant::now();
        for _ in 0..MAKEROOM_SCAN_BUDGET {
            let Some(key) = self.order.pop_front() else {
                return true;
            };
            self.swept += 1;
            let Some(bucket) = self.buckets.get_mut(&key) else {
                return true;
            };
            let elapsed = now.duration_since(bucket.last).as_secs_f64().max(0.0);
            bucket.tokens = (bucket.tokens + elapsed * refill).min(burst);
            bucket.last = now;
            if bucket.tokens >= burst {
                self.buckets.remove(&key);
                return true;
            }
            self.order.push_back(key);
        }
        false
    }
}

impl IpRateLimiter {
    pub fn allow(&mut self, addr: IpAddr) -> bool {
        self.ips.charge(addr, 1.0)
    }

    pub fn allow_player(&mut self, uid: Uuid) -> bool {
        self.players.charge(uid, 1.0)
    }
}
