use std::net::IpAddr;
use std::time::Duration;

use super::SourceWalk;
use crate::infra::rate_limit::*;

fn flood_exhausted(l: &mut IpRateLimiter) {
    let mut walk = SourceWalk { n: 0 };
    let mut n = 0u32;
    while l.tracked() < MAX_BUCKETS && n < 200_000 {
        n += 1;
        l.allow(walk.next());
    }
}

#[test]
fn the_ceiling_drops_refilled_buckets_rather_than_exhausted_ones() {
    let mut l = IpRateLimiter::with_rates(0.0001, 1.0);
    let first = IpAddr::from([10, 0, 0, 1]);
    assert!(l.allow(first));
    assert!(!l.allow(first), "first is exhausted");
    flood_exhausted(&mut l);
    assert_eq!(l.tracked(), MAX_BUCKETS);
    assert!(
        !l.allow(first),
        "an exhausted source must not be handed a fresh bucket by eviction"
    );
    assert_eq!(l.ips.order.len(), l.tracked(), "the queue tracks the map");
    assert_eq!(l.squeezed(), 0, "first had a bucket, it was just empty");
}

#[test]
fn a_new_source_at_a_full_ceiling_is_refused_and_counted_apart() {
    let mut l = IpRateLimiter::with_rates(0.0001, 1.0);
    flood_exhausted(&mut l);
    assert_eq!(l.tracked(), MAX_BUCKETS);
    let latecomer = IpAddr::from([8, 8, 8, 8]);
    assert!(!l.allow(latecomer), "no room, no bucket created");
    assert_eq!(l.tracked(), MAX_BUCKETS, "the ceiling holds");
    assert_eq!(l.squeezed(), 1);
    assert!(
        l.retry_after(latecomer) >= 1,
        "and it is not told to go now"
    );
}

#[test]
fn a_full_ceiling_clears_itself_once_sources_refill() {
    let mut l = IpRateLimiter::with_rates(1000.0, 1.0);
    flood_exhausted(&mut l);
    std::thread::sleep(Duration::from_millis(20));
    let latecomer = IpAddr::from([8, 8, 8, 8]);
    assert!(l.allow(latecomer), "idle buckets were swept, room exists");
    assert!(l.tracked() <= MAX_BUCKETS);
}
