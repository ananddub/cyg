use std::net::IpAddr;
use std::sync::Arc;

use super::ip;
use crate::infra::rate_limit::*;

/// Nine sources hit the limiter at the same time from separate threads. The
/// hot source burns its burst and is cut off; the other eight each stay under
/// their own budget, no matter which shard the FNV hash sent them to. With a
/// negligible refill rate the arithmetic is exact, so the summed counters
/// prove the per-IP semantics survived sharding.
#[test]
fn concurrent_sources_are_limited_per_ip_not_per_lock() {
    const BURST: u32 = 50;
    const HOT_CALLS: usize = 300;
    const CALM_CALLS: usize = 10;
    const CALM_IPS: usize = 8;

    // A rate so low the test window refills nothing: the arithmetic is exact.
    let l = Arc::new(IpRateLimiter::with_rates(0.0001, f64::from(BURST)));
    let hot = ip(1);
    let calm: Vec<IpAddr> = (2..(2 + CALM_IPS as u8)).map(ip).collect();

    // Four hot threads, eight calm threads, and this test function all line
    // up on the same barrier so the hits actually overlap.
    let barrier = Arc::new(std::sync::Barrier::new(4 + CALM_IPS + 1));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let l = Arc::clone(&l);
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut allowed = 0usize;
            for _ in 0..HOT_CALLS / 4 {
                if l.allow(hot) {
                    allowed += 1;
                }
            }
            (true, allowed)
        }));
    }
    for calm_ip in &calm {
        let l = Arc::clone(&l);
        let barrier = Arc::clone(&barrier);
        let calm_ip = *calm_ip;
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut allowed = 0usize;
            for _ in 0..CALM_CALLS {
                if l.allow(calm_ip) {
                    allowed += 1;
                }
            }
            (false, allowed)
        }));
    }

    barrier.wait();

    let mut hot_allowed = 0usize;
    let mut calm_allowed = 0usize;
    for handle in handles {
        let (hot, allowed) = handle.join().expect("test threads do not panic");
        if hot {
            hot_allowed += allowed;
        } else {
            assert_eq!(
                allowed, CALM_CALLS,
                "a calm source always fits under its own burst"
            );
            calm_allowed += allowed;
        }
    }

    assert_eq!(
        hot_allowed, BURST as usize,
        "the hot source got exactly its burst, from all four threads"
    );
    assert_eq!(
        calm_allowed,
        CALM_CALLS * CALM_IPS,
        "each calm source spent its own budget untouched"
    );
    assert_eq!(l.tracked(), 1 + CALM_IPS, "one bucket per source");
    let (allowed_total, denied_total) = l.counts();
    assert_eq!(
        allowed_total,
        (hot_allowed + calm_allowed) as u64,
        "the counters sum across the shards"
    );
    assert_eq!(
        denied_total,
        (HOT_CALLS - hot_allowed) as u64,
        "the hot source's overflow was refused, not shifted onto others"
    );
    assert!(
        !l.allow(hot),
        "the hot source stays cut off once its burst is gone"
    );
}
