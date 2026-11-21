use std::net::IpAddr;

use uuid::Uuid;

use super::bucket::{
    Bucket, IpRateLimiter, MAKEROOM_SCAN_BUDGET, Store, lock, shard_of, shard_of_uuid,
};

impl<K: Eq + std::hash::Hash + Clone> Store<K> {
    /// Takes `amount` tokens for `key`, creating its bucket when it is new.
    pub(super) fn charge(&mut self, key: K, amount: f64) -> bool {
        if !self.enabled() {
            return true;
        }
        let now = std::time::Instant::now();
        let burst = self.burst;
        let refill = self.refill_per_sec;
        // Capacity check BEFORE taking the entry borrow (make_room mutates
        // the same map). Hits only on the rare full-table path.
        if !self.buckets.contains_key(&key)
            && self.buckets.len() >= self.max_buckets
            && !self.make_room()
        {
            self.denied += 1;
            self.squeezed += 1;
            return false;
        }
        // Entry API: one hash + one key clone — the old get_mut/insert/get_mut
        // rehashed twice and cloned twice per charge.
        match self.buckets.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occ) => {
                let bucket = occ.get_mut();
                if bucket.tokens < burst {
                    let elapsed = now.duration_since(bucket.last).as_secs_f64();
                    bucket.tokens = (bucket.tokens + elapsed * refill).min(burst);
                    bucket.last = now;
                }
                if bucket.tokens >= amount {
                    bucket.tokens -= amount;
                    self.allowed += 1;
                    true
                } else {
                    self.denied += 1;
                    false
                }
            }
            std::collections::hash_map::Entry::Vacant(vac) => {
                let bucket = vac.insert(Bucket {
                    tokens: burst,
                    last: now,
                });
                bucket.tokens -= amount; // a fresh bucket starts full
                self.order.push_back(key);
                self.allowed += 1;
                true
            }
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
    pub fn allow(&self, addr: IpAddr) -> bool {
        lock(self.shard(&addr)).charge(addr, 1.0)
    }

    pub fn allow_player(&self, uid: Uuid) -> bool {
        lock(&self.players[shard_of_uuid(&uid)]).charge(uid, 1.0)
    }

    /// The password-route bucket: one attempt costs one token from a table
    /// that refills `AUTH_REFILL_PER_SEC` per second, so password guessing
    /// from one address stays slow no matter how the plane's general bucket
    /// is configured.
    pub fn allow_auth(&self, addr: IpAddr) -> bool {
        lock(&self.auth[shard_of(&addr)]).charge(addr, 1.0)
    }
}
