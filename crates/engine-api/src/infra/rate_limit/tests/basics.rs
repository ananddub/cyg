use super::ip;
use crate::infra::rate_limit::*;
use std::net::IpAddr;
use std::time::Duration;

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
fn ipv6_sources_do_not_share_a_bucket() {
    let mut l = IpRateLimiter::new(1, 1);
    let a = IpAddr::from([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1]);
    let b = IpAddr::from([0x2001, 0xdb8, 0, 0, 0, 0, 0, 2]);
    assert!(l.allow(a));
    assert!(l.allow(b));
    assert!(!l.allow(a));
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
    assert!(l.retry_after(v6) >= 1);
}
