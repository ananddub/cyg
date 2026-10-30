use super::{SourceWalk, flood_to_ceiling};
use crate::infra::rate_limit::bucket::MAKEROOM_SCAN_BUDGET;
use crate::infra::rate_limit::*;

#[test]
fn bucket_count_and_queue_are_both_bounded() {
    let mut l = IpRateLimiter::new(1000, 1);
    let mut walk = SourceWalk { n: 0 };
    for _ in 0..(MAX_BUCKETS + MAX_BUCKETS / 2) {
        l.allow(walk.next());
    }
    assert!(l.tracked() <= MAX_BUCKETS, "tracked {}", l.tracked());
    assert_eq!(l.ips.order.len(), l.tracked(), "one queue entry per bucket");
    assert!(l.tracked() > 0, "reclaiming does not empty the limiter");
    let mut slow = IpRateLimiter::with_rates(0.0001, 1.0);
    let mut walk = SourceWalk { n: 0 };
    for _ in 0..(MAX_BUCKETS + MAX_BUCKETS / 2) {
        slow.allow(walk.next());
    }
    assert_eq!(slow.tracked(), MAX_BUCKETS);
    assert_eq!(slow.ips.order.len(), slow.tracked());
}

#[test]
fn a_flood_at_the_ceiling_costs_bounded_work_per_request() {
    let (mut l, mut walk) = flood_to_ceiling();
    let before = l.swept();
    for _ in 0..1000 {
        assert!(!l.allow(walk.next()), "no room, so no admission");
    }
    let per_request = (l.swept() - before) / 1000;
    assert!(
        per_request <= MAKEROOM_SCAN_BUDGET as u64,
        "an admitted spoof cost {per_request} bucket reads, budget is {MAKEROOM_SCAN_BUDGET}"
    );
    assert_eq!(
        l.tracked(),
        MAX_BUCKETS,
        "and nobody's bucket was displaced"
    );
}

#[test]
fn a_reclaimable_slot_is_found_even_at_the_back_of_the_rotation() {
    let (mut l, mut walk) = flood_to_ceiling();
    let stale = *l.ips.order.back().unwrap();
    let burst = l.ips.burst;
    l.ips.buckets.get_mut(&stale).unwrap().tokens = burst;
    let limit = (MAX_BUCKETS / MAKEROOM_SCAN_BUDGET) as u32 + 2;
    let mut admissions = 0u32;
    while l.ips.buckets.contains_key(&stale) {
        admissions += 1;
        assert!(admissions <= limit, "the rotation never reached the slot");
        l.allow(walk.next());
    }
    assert!(l.tracked() <= MAX_BUCKETS, "the ceiling held throughout");
    assert!(
        admissions > 1,
        "the ring had to walk to it, not trip over it"
    );
    assert_eq!(l.squeezed(), u64::from(admissions - 1));
}
