use std::net::IpAddr;

use super::bucket::{Bucket, IpRateLimiter, MAKEROOM_SCAN_BUDGET, MAX_BUCKETS};

impl IpRateLimiter {
    /// Take one token; `false` means this source is over budget right now. The key is the
    /// *exact* `IpAddr` of the peer socket, never normalised: `127.0.0.1`, `::1` and
    /// `::ffff:127.0.0.1` are three buckets, so no address family can spend another's budget.
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
                // No room and nothing reclaimable in this budget: refuse the
                // *new* source instead of displacing an exhausted one. Evicting
                // an exhausted bucket would hand its address a full burst on the
                // next call, which turns the ceiling into a rate-limit bypass.
                self.denied += 1;
                self.squeezed += 1;
                return false;
            }
            self.buckets.insert(addr, Bucket { tokens: burst, last: now });
            self.order.push_back(addr);
        }
        let bucket = self.buckets.get_mut(&addr).expect("bucket was just created");
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            self.allowed += 1;
            true
        } else {
            self.denied += 1;
            false
        }
    }

    /// Reclaim one slot at the ceiling, examining at most [`MAKEROOM_SCAN_BUDGET`] buckets,
    /// and say whether a slot is free. `order` is a ring: a candidate back to a full burst is
    /// dropped (it was going to be allowed anyway); one still below a full burst is a
    /// genuinely overspent source, put back untouched.
    fn make_room(&mut self) -> bool {
        let burst = self.burst;
        let refill = self.refill_per_sec;
        let now = std::time::Instant::now();
        for _ in 0..MAKEROOM_SCAN_BUDGET {
            // The queue is empty, so the map is too: nothing to reclaim, but there
            // is room for a new source.
            let Some(addr) = self.order.pop_front() else { return true };
            self.swept += 1;
            let Some(bucket) = self.buckets.get_mut(&addr) else {
                // A queue entry with no bucket is a slot that is already free.
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
