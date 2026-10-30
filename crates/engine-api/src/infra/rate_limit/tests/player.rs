use uuid::Uuid;

use crate::infra::rate_limit::*;

#[test]
fn an_unconfigured_account_bucket_admits_everything() {
    let mut l = IpRateLimiter::new(1000, 1000);
    let uid = Uuid::new_v4();
    for _ in 0..100 {
        assert!(l.allow_player(uid), "no rates, no ceiling");
    }
    assert_eq!(l.player_counts(), (0, 0), "nothing was actually bucketed");
    assert_eq!(l.tracked_players(), 0);
}

#[test]
fn one_account_cannot_borrow_another_s_budget() {
    let mut l = IpRateLimiter::new(1000, 1000);
    l.set_player_rates(1, 2);
    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    assert!(l.allow_player(first));
    assert!(l.allow_player(first));
    assert!(!l.allow_player(first), "its burst is spent");
    assert!(
        l.allow_player(second),
        "the other account has its own bucket"
    );
    assert_eq!(l.player_counts(), (3, 1));
    assert_eq!(l.counts(), (0, 0), "the per-source table never saw these");
}

#[test]
fn the_account_bucket_answers_with_a_wait() {
    let mut l = IpRateLimiter::new(1000, 1000);
    let uid = Uuid::new_v4();
    assert_eq!(l.retry_after_player(uid), 0, "off, so proceed");
    l.set_player_rates(2, 1);
    assert!(l.allow_player(uid));
    assert!(!l.allow_player(uid));
    assert_eq!(l.retry_after_player(uid), 1, "one token at 2/s rounds up");
}
