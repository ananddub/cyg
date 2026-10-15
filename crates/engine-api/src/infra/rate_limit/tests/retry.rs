use super::ip;
use crate::infra::rate_limit::*;

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

    // Same deficit at 5/s is a second, and a source that still has a token is told nothing.
    let mut fast = IpRateLimiter::with_rates(5.0, 5.0);
    assert_eq!(fast.retry_after(ip(1)), 0, "never seen");
    for _ in 0..5 {
        assert!(fast.allow(ip(1)));
    }
    assert_eq!(fast.retry_after(ip(1)), 1, "1 token at 5/s");
    assert_ne!(
        slow.retry_after(ip(1)),
        fast.retry_after(ip(1)),
        "the answer must depend on the bucket, not on a fixed default"
    );
}

/// Whatever the limiter advertises, a source that waits that long must then be
/// allowed: the value is a ceiling-rounded deficit, never a floor.
#[test]
fn retry_after_never_understates_the_wait() {
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
