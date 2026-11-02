use super::*;

#[test]
fn fixed_window_counts_and_disconnects() {
    let mut guard = MessageRateGuard::new(3);
    let t0 = Instant::now();
    assert_eq!(guard.allow(t0), RateDecision::Allow);
    assert_eq!(guard.allow(t0), RateDecision::Allow);
    assert_eq!(guard.allow(t0), RateDecision::Allow);
    assert_eq!(guard.allow(t0), RateDecision::Disconnect);
    assert_eq!(guard.allow(t0), RateDecision::Disconnect);
}

#[test]
fn window_reset_restores_budget_without_credit_carry() {
    let mut guard = MessageRateGuard::new(2);
    let t0 = Instant::now();
    assert_eq!(guard.allow(t0), RateDecision::Allow);
    assert_eq!(guard.allow(t0), RateDecision::Allow);
    assert_eq!(guard.allow(t0), RateDecision::Disconnect);

    let t1 = t0 + Duration::from_millis(1001);
    assert_eq!(guard.allow(t1), RateDecision::Allow);
    assert_eq!(guard.allow(t1), RateDecision::Allow);
    assert_eq!(guard.allow(t1), RateDecision::Disconnect);
}

#[test]
fn zero_limit_disables_everything() {
    let mut limits = MessageRateLimits::new(0);
    let t0 = Instant::now();
    for _ in 0..10_000 {
        assert_eq!(limits.check("spammer", t0), RateDecision::Allow);
    }
    assert!(limits.is_empty(), "disabled limiter keeps no state");
}

#[test]
fn exceeded_session_is_dropped_immediately() {
    let mut limits = MessageRateLimits::new(2);
    let t0 = Instant::now();
    assert_eq!(limits.check("a", t0), RateDecision::Allow);
    assert_eq!(limits.check("a", t0), RateDecision::Allow);
    assert_eq!(limits.check("a", t0), RateDecision::Disconnect);
    assert!(limits.is_empty(), "violator state is dropped with the seat");
}

#[test]
fn sessions_are_independent() {
    let mut limits = MessageRateLimits::new(2);
    let t0 = Instant::now();
    assert_eq!(limits.check("a", t0), RateDecision::Allow);
    assert_eq!(limits.check("b", t0), RateDecision::Allow);
    assert_eq!(limits.check("a", t0), RateDecision::Allow);
    assert_eq!(limits.check("a", t0), RateDecision::Disconnect);
    assert_eq!(limits.check("b", t0), RateDecision::Allow);
    assert_eq!(limits.check("b", t0), RateDecision::Disconnect);
}

#[test]
fn sweep_drops_only_idle_sessions() {
    let mut limits = MessageRateLimits::new(100);
    let t0 = Instant::now();
    limits.check("active", t0);
    limits.check("gone", t0);

    limits.sweep(t0);
    assert_eq!(limits.len(), 2);

    let mut t = t0;
    for _ in 0..36 {
        t += Duration::from_secs(1);
        limits.check("active", t);
        limits.sweep(t);
    }
    assert_eq!(limits.len(), 1, "only the active session remains");
    limits.remove("active");
    assert!(limits.is_empty());
}
