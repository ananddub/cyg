use std::net::IpAddr;

use super::bucket::{Bucket, IpRateLimiter, MAKEROOM_SCAN_BUDGET, MAX_BUCKETS};

impl IpRateLimiter {
    pub fn allow(&mut self, addr: IpAddr) -> bool {
        let now = std::time::Instant::now();
        let burst = self.burst;
        let refill = self.refill_per_sec;
        if let Some(bucket) = self.buckets.get_mut(&addr) {
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
                addr,
                Bucket {
                    tokens: burst,
                    last: now,
                },
            );
            self.order.push_back(addr);
        }
        let bucket = self
            .buckets
            .get_mut(&addr)
            .expect("bucket was just created");
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            self.allowed += 1;
            true
        } else {
            self.denied += 1;
            false
        }
    }

    fn make_room(&mut self) -> bool {
        let burst = self.burst;
        let refill = self.refill_per_sec;
        let now = std::time::Instant::now();
        for _ in 0..MAKEROOM_SCAN_BUDGET {
            let Some(addr) = self.order.pop_front() else {
                return true;
            };
            self.swept += 1;
            let Some(bucket) = self.buckets.get_mut(&addr) else {
                return true;
            };
            let elapsed = now.duration_since(bucket.last).as_secs_f64().max(0.0);
            bucket.tokens = (bucket.tokens + elapsed * refill).min(burst);
            bucket.last = now;
            if bucket.tokens >= burst {
                self.buckets.remove(&addr);
                return true;
            }
            self.order.push_back(addr);
        }
        false
    }
}
