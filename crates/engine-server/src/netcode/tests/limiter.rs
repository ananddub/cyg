use std::time::Instant;

use crate::netcode::*;

#[test]
fn rate_limiter_rejects_a_flood() {
    let mut limiter = InputRateLimiter::new(10, 2.0);
    let now = Instant::now();
    assert!(limiter.allow(now));
    assert!(limiter.allow(now));
    assert!(!limiter.allow(now), "burst exhausted");
    assert!(limiter.allow(now + std::time::Duration::from_millis(250)));
}

#[test]
fn the_input_budget_is_sized_from_the_tick_rate() {
    let mut limiter = InputRateLimiter::for_tick_rate(30);
    let now = Instant::now();
    let allowed = (0..1_000).filter(|_| limiter.allow(now)).count();
    assert_eq!(allowed, 60, "burst is a quarter second of samples");
    assert!(
        limiter.allow(now + std::time::Duration::from_millis(500)),
        "half a second of refill should accept again"
    );
}

#[test]
fn a_batch_is_charged_whole_or_refused_whole() {
    let mut limiter = InputRateLimiter::new(10, 5.0);
    let now = std::time::Instant::now();
    assert!(limiter.allow_n(now, 4));
    assert_eq!(limiter.allow_n(now, 2), false, "not enough for the whole batch");
    assert!(limiter.allow_n(now, 1), "a partial charge never happened");
}
