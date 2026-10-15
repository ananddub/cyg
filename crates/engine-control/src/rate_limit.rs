//! Per-source token bucket.
//!
//! Matchmake is the one endpoint an unauthenticated client can hammer, and each
//! call costs an HMAC plus a map entry, so the abuse cost lands on the server.
//! One bucket per client IP, refilled lazily on access — no timer, no
//! background task, and the whole structure is one `HashMap` behind a mutex.
//!
//! Only the request's *socket* address is charged. A `X-Forwarded-For` header
//! is client-supplied and would let a caller pick which bucket it bypasses; if
//! this plane is ever put behind a proxy, the proxy becomes the rate limiter.
//!
//! Memory is bounded the same way the seat map is: at most [`MAX_BUCKETS`]
//! addresses are tracked, so a source-address flood costs a fixed hash table
//! rather than an unbounded one. Making room at that ceiling is a *bounded*
//! rotation of the tracked set, never a full sweep, because a ceiling whose
//! admission path is O(tracked sources) is a denial-of-service against the
//! limiter itself.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;

/// Distinct buckets kept before the ceiling bites. A NAT box full of browsers
/// must not be able to grow this map without bound, and a source-address flood
/// must not be able to *shrink* anyone else's bucket either: at the ceiling,
/// refilled buckets are dropped and a brand-new source is refused rather than
/// evicting somebody's exhausted bucket (which would hand them a free reset).
pub const MAX_BUCKETS: usize = 16384;

/// Buckets examined per [`IpRateLimiter::make_room`] call. The ceiling has to
/// bite in constant time: sweeping all of `MAX_BUCKETS` on every admission made
/// the limiter itself the most expensive object under a spoofed-source flood,
/// which is exactly the traffic it exists to survive. A bounded rotation still
/// reaches every tracked bucket within `MAX_BUCKETS / MAKEROOM_SCAN_BUDGET`
/// admissions, so a reclaimable slot is found — just not on a platter.
const MAKEROOM_SCAN_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: f64,
    last: std::time::Instant,
}

pub struct IpRateLimiter {
    refill_per_sec: f64,
    burst: f64,
    buckets: HashMap<IpAddr, Bucket>,
    /// First-seen order, used as a rotation ring by [`IpRateLimiter::make_room`].
    /// One entry per *tracked IP*, so it stays bounded by `MAX_BUCKETS` no
    /// matter how many requests arrive.
    order: VecDeque<IpAddr>,
    allowed: u64,
    denied: u64,
    /// Requests refused because the tracked-source ceiling was full. A capacity
    /// event, not an abuse event: it is counted apart from `denied` so the two
    /// can be told apart downstream.
    squeezed: u64,
    /// Buckets examined while making room at the ceiling. This is the work the
    /// limiter does on behalf of a flood, and [`IpRateLimiter::make_room`] keeps
    /// it bounded per call; the counter exists so a test can prove that.
    swept: u64,
}

impl IpRateLimiter {
    pub fn new(refill_per_sec: u32, burst: u32) -> IpRateLimiter {
        IpRateLimiter::with_rates(f64::from(refill_per_sec), f64::from(burst))
    }

    /// The same bucket with a fractional rate. `new` takes whole requests per
    /// second because that is what `[control]` configures; this exists so a
    /// deployment can police below 1 rps and so `retry_after` can be tested
    /// against rates where the honest answer is not trivially one second.
    pub fn with_rates(refill_per_sec: f64, burst: f64) -> IpRateLimiter {
        IpRateLimiter {
            refill_per_sec: refill_per_sec.max(f64::EPSILON),
            burst: burst.max(1.0),
            buckets: HashMap::new(),
            order: VecDeque::new(),
            allowed: 0,
            denied: 0,
            squeezed: 0,
            swept: 0,
        }
    }

    /// Take one token. `false` means this source is over budget right now; the
    /// handler turns that into a 429 plus a `Retry-After`.
    ///
    /// The key is the *exact* `IpAddr` of the peer socket, never normalised:
    /// `127.0.0.1`, `::1` and the IPv4-mapped `::ffff:127.0.0.1` are three
    /// different sources with three different buckets, so no address family can
    /// spend another family's budget (or borrow its deny).
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

    /// Reclaim one slot at the ceiling, examining at most
    /// [`MAKEROOM_SCAN_BUDGET`] buckets, and say whether a slot is free.
    ///
    /// `order` is a ring. Each candidate is refilled lazily and, if it has come
    /// back to a full burst, dropped: losing that source costs nothing, since it
    /// was going to be allowed anyway. A bucket still below a full burst goes
    /// back on the queue untouched — it is a source that is genuinely over
    /// budget, which is the set worth remembering, and evicting it would be the
    /// bypass described in [`IpRateLimiter::allow`].
    ///
    /// The budget is the point, not an optimisation on the side. A full-table
    /// sweep made every admission at the ceiling cost O(`MAX_BUCKETS`) hash
    /// operations, so the limiter was the most expensive object under exactly the
    /// spoofed-source flood it exists to survive. Rotating a bounded number of
    /// entries per call keeps admission O(1) and still walks the whole tracked
    /// set, so a reclaimable slot is found — within one rotation rather than
    /// one call.
    fn make_room(&mut self) -> bool {
        let burst = self.burst;
        let refill = self.refill_per_sec;
        let now = std::time::Instant::now();
        for _ in 0..MAKEROOM_SCAN_BUDGET {
            // The queue is empty, so the map is too: nothing to reclaim, but
            // there is room for a new source.
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

    /// Whole seconds this source must wait for its next token — the value put
    /// in `Retry-After`.
    ///
    /// Derived from the bucket, not from a constant: the deficit (how far below
    /// one token this source is) divided by the refill rate, rounded *up* so a
    /// client that obeys it is never refused again. `0` means this source can
    /// proceed right now, which keeps a stale `Retry-After` from sticking to a
    /// healthy address. Refill is applied as of this read, so a caller that
    /// waits the advertised time sees a smaller number, never a larger one.
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
    /// Exposed for `/status`-style diagnostics and for the honesty test below;
    /// the deny/allow decision itself uses the mutable path in [`allow`].
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

    /// Total buckets examined by the ceiling's reclaim path. The per-call bound
    /// is what keeps a flood cheap, so this is the number that shows the ceiling
    /// is being paid for in the right currency.
    pub fn swept(&self) -> u64 {
        self.swept
    }

    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([127, 0, 0, n])
    }

    #[test]
    fn a_burst_up_to_limit_passes_then_it_denies() {
        let mut l = IpRateLimiter::new(1, 3);
        for _ in 0..3 {
            assert!(l.allow(ip(1)));
        }
        assert!(!l.allow(ip(1)), "the 4th in a row is over budget");
        assert!(l.allow(ip(2)), "a different source has its own bucket");
    }

    #[test]
    fn tokens_come_back_with_time() {
        let mut l = IpRateLimiter::new(50, 1);
        assert!(l.allow(ip(1)));
        assert!(!l.allow(ip(1)));
        std::thread::sleep(Duration::from_millis(40));
        assert!(l.allow(ip(1)));
    }

    #[test]
    fn a_fresh_source_starts_full_not_empty() {
        let mut l = IpRateLimiter::new(1, 5);
        for _ in 0..5 {
            assert!(l.allow(ip(7)), "a fresh bucket holds the whole burst");
        }
        assert!(!l.allow(ip(7)), "and nothing more");
        assert_eq!(l.counts(), (5, 1));
    }

    #[test]
    fn ipv6_sources_do_not_share_a_bucket() {
        let mut l = IpRateLimiter::new(1, 1);
        let a = IpAddr::from([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1]);
        let b = IpAddr::from([0x2001, 0xdb8, 0, 0, 0, 0, 0, 2]);
        assert!(l.allow(a));
        assert!(l.allow(b));
        assert!(!l.allow(a));
    }

    /// A distinct address per call, so a flood can be generated without a
    /// nested loop that quietly decides how much work the test does.
    struct SourceWalk {
        n: u32,
    }

    impl SourceWalk {
        fn next(&mut self) -> IpAddr {
            self.n += 1;
            IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0000u32 + self.n))
        }
    }

    #[test]
    fn bucket_count_and_queue_are_both_bounded() {
        // Fast refill: buckets come back to a full burst and are reclaimable, so
        // the tracked set churns but never grows past the ceiling.
        let mut l = IpRateLimiter::new(1000, 1);
        let mut walk = SourceWalk { n: 0 };
        for _ in 0..(MAX_BUCKETS + MAX_BUCKETS / 2) {
            l.allow(walk.next());
        }
        assert!(l.tracked() <= MAX_BUCKETS, "tracked {}", l.tracked());
        assert_eq!(l.order.len(), l.tracked(), "one queue entry per bucket");
        assert!(l.tracked() > 0, "reclaiming does not empty the limiter");
        // No refill at test speed: every source is genuinely exhausted, so the
        // ceiling is pinned exactly and the queue still mirrors the map.
        let mut slow = IpRateLimiter::with_rates(0.0001, 1.0);
        let mut walk = SourceWalk { n: 0 };
        for _ in 0..(MAX_BUCKETS + MAX_BUCKETS / 2) {
            slow.allow(walk.next());
        }
        assert_eq!(slow.tracked(), MAX_BUCKETS);
        assert_eq!(slow.order.len(), slow.tracked());
    }

    #[test]
    fn a_flood_at_the_ceiling_costs_bounded_work_per_request() {
        // The property the reclaim budget exists for: with the table full of
        // exhausted sources, each *additional* spoofed address may not cost a
        // walk of the table. If it did, the limiter would be the most expensive
        // object under exactly the traffic it is meant to survive.
        let mut l = IpRateLimiter::with_rates(0.0001, 1.0);
        let mut walk = SourceWalk { n: 0 };
        while l.tracked() < MAX_BUCKETS {
            l.allow(walk.next());
        }
        let before = l.swept();
        for _ in 0..1000 {
            assert!(!l.allow(walk.next()), "no room, so no admission");
        }
        let per_request = (l.swept() - before) / 1000;
        assert!(
            per_request <= MAKEROOM_SCAN_BUDGET as u64,
            "an admitted spoof cost {per_request} bucket reads, budget is {MAKEROOM_SCAN_BUDGET}"
        );
        assert_eq!(l.tracked(), MAX_BUCKETS, "and nobody's bucket was displaced");
    }

    #[test]
    fn a_reclaimable_slot_is_found_even_at_the_back_of_the_rotation() {
        // The budget must not turn the ceiling into a latch: one refilled bucket
        // is enough to keep admitting, even when the rotation has to walk to it.
        let mut l = IpRateLimiter::with_rates(0.0001, 1.0);
        let mut walk = SourceWalk { n: 0 };
        while l.tracked() < MAX_BUCKETS {
            l.allow(walk.next());
        }
        // The last entry in the ring is the worst case for a bounded scan.
        let stale = *l.order.back().unwrap();
        let burst = l.burst;
        l.buckets.get_mut(&stale).unwrap().tokens = burst; // refilled: free to forget
        let limit = (MAX_BUCKETS / MAKEROOM_SCAN_BUDGET) as u32 + 2;
        let mut admissions = 0u32;
        while l.buckets.contains_key(&stale) {
            admissions += 1;
            assert!(admissions <= limit, "the rotation never reached the slot");
            l.allow(walk.next());
        }
        assert!(l.tracked() <= MAX_BUCKETS, "the ceiling held throughout");
        assert!(admissions > 1, "the ring had to walk to it, not trip over it");
        // Every call before the reclaim was a counted capacity refusal, never a
        // silent eviction of someone still over budget.
        assert_eq!(l.squeezed(), u64::from(admissions - 1));
    }

    #[test]
    fn retry_after_is_zero_only_for_a_source_that_can_proceed() {
        let l = IpRateLimiter::new(2, 1);
        assert_eq!(l.retry_after(ip(9)), 0, "unknown source");
        let mut l = IpRateLimiter::new(2, 1);
        assert!(l.allow(ip(9)));
        assert!(!l.allow(ip(9)));
        assert_eq!(l.retry_after(ip(9)), 1);
    }

    #[test]
    fn retry_after_tracks_the_bucket_instead_of_a_constant() {
        // 0.1 tokens/second: one missing token is honestly ten seconds away.
        let mut slow = IpRateLimiter::with_rates(0.1, 1.0);
        assert!(slow.allow(ip(1)));
        assert!(!slow.allow(ip(1)));
        assert_eq!(slow.retry_after(ip(1)), 10, "deficit / refill rate");

        // Same deficit at 5/s is a second, and a source that still has a token
        // is told nothing at all.
        let mut fast = IpRateLimiter::with_rates(5.0, 5.0);
        assert_eq!(fast.retry_after(ip(1)), 0, "never seen");
        for _ in 0..5 {
            assert!(fast.allow(ip(1)));
        }
        assert_eq!(fast.retry_after(ip(1)), 1, "1 token at 5/s");
        assert_ne!(
            slow.retry_after(ip(1)), fast.retry_after(ip(1)),
            "the answer must depend on the bucket, not on a fixed default"
        );
    }

    #[test]
    fn retry_after_never_understates_the_wait() {
        // Whatever the limiter advertises, a source that waits that long must
        // then be allowed: the value is a ceiling-rounded deficit, never a
        // floor, and it is computed against the same refill the deny used.
        for rate in [0.25, 0.5, 1.0, 4.0] {
            let mut l = IpRateLimiter::with_rates(rate, 1.0);
            assert!(l.allow(ip(3)));
            assert!(!l.allow(ip(3)));
            let advertised = l.retry_after(ip(3)) as f64;
            assert!(
                advertised * rate >= 1.0 - l.tokens_now(ip(3)),
                "rate {rate}: {advertised}s is shorter than the deficit"
            );
        }
    }

    #[test]
    fn a_denied_request_costs_no_tokens_and_is_counted() {
        let mut l = IpRateLimiter::new(1, 2);
        assert!(l.allow(ip(4)));
        assert!(l.allow(ip(4)));
        for _ in 0..10 {
            assert!(!l.allow(ip(4)));
        }
        assert_eq!(l.counts(), (2, 10));
        assert!(
            l.tokens_now(ip(4)) < 1.0,
            "a refusal cannot top the bucket back up"
        );
    }

    #[test]
    fn ipv4_and_ipv6_and_mapped_ipv4_never_share_a_bucket() {
        let mut l = IpRateLimiter::with_rates(1.0, 1.0);
        let v4 = IpAddr::from([203, 0, 113, 7]);
        let v6 = IpAddr::from([0x2001, 0xdb8, 0, 0, 0, 0, 0, 7]);
        let mapped = IpAddr::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 203, 0, 113, 7]);
        assert_eq!(mapped.to_string(), "::ffff:203.0.113.7");
        for addr in [v4, v6, mapped] {
            assert!(l.allow(addr), "{addr} spends its own burst");
            assert!(!l.allow(addr), "{addr} is exhausted on its own");
        }
        assert_eq!(l.tracked(), 3, "three sources, three buckets");
        // Exhausting the IPv4 address must not lock out the mapped form that a
        // dual-stack listener hands back for the same client — they are
        // separate keys on purpose, so the rule is "never share", not "merge".
        assert!(l.retry_after(v6) >= 1);
    }

    #[test]
    fn the_ceiling_drops_refilled_buckets_rather_than_exhausted_ones() {
        // 0.0001 tokens/s: nothing refills while this test runs, so every
        // tracked source is genuinely exhausted.
        let mut l = IpRateLimiter::with_rates(0.0001, 1.0);
        let first = IpAddr::from([10, 0, 0, 1]);
        assert!(l.allow(first));
        assert!(!l.allow(first), "first is exhausted");
        let mut n = 0u32;
        while l.tracked() < MAX_BUCKETS && n < 200_000 {
            n += 1;
            l.allow(IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0000u32 + n)));
        }
        assert_eq!(l.tracked(), MAX_BUCKETS);
        assert!(
            !l.allow(first),
            "an exhausted source must not be handed a fresh bucket by eviction"
        );
        assert_eq!(l.order.len(), l.tracked(), "the queue tracks the map");
        assert_eq!(l.squeezed(), 0, "first had a bucket, it was just empty");
    }

    #[test]
    fn a_new_source_at_a_full_ceiling_is_refused_and_counted_apart() {
        let mut l = IpRateLimiter::with_rates(0.0001, 1.0); // nothing refills
        let mut n = 0u32;
        while l.tracked() < MAX_BUCKETS && n < 200_000 {
            n += 1;
            l.allow(IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0000u32 + n)));
        }
        assert_eq!(l.tracked(), MAX_BUCKETS);
        let latecomer = IpAddr::from([8, 8, 8, 8]);
        assert!(!l.allow(latecomer), "no room, no bucket created");
        assert_eq!(l.tracked(), MAX_BUCKETS, "the ceiling holds");
        assert_eq!(l.squeezed(), 1);
        assert!(l.retry_after(latecomer) >= 1, "and it is not told to go now");
    }

    #[test]
    fn a_full_ceiling_clears_itself_once_sources_refill() {
        // The squeeze must be self-healing: a sweep of refilled buckets makes
        // room again without anyone's help.
        let mut l = IpRateLimiter::with_rates(1000.0, 1.0);
        let mut n = 0u32;
        while l.tracked() < MAX_BUCKETS && n < 200_000 {
            n += 1;
            l.allow(IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0000u32 + n)));
        }
        std::thread::sleep(Duration::from_millis(20)); // 20 tokens' worth
        let latecomer = IpAddr::from([8, 8, 8, 8]);
        assert!(l.allow(latecomer), "idle buckets were swept, room exists");
        assert!(l.tracked() <= MAX_BUCKETS);
    }
}
