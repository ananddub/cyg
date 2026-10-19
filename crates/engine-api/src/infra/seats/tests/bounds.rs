use crate::infra::seats::*;

#[test]
fn the_drift_cap_refuses_instead_of_handing_out_forever_seats() {
    let mut r = SeatRegistry::new("secret");
    let mut count = 0;
    while r.mint("tank-3d", 1).is_some() {
        count += 1;
        if count > MAX_TTL_DRIFT_SECS + 10 {
            panic!("minting exceeded drift cap");
        }
    }
    assert!(count >= MAX_TTL_DRIFT_SECS + 1);
    assert_eq!(r.refused(), 1);
    // Another room is unaffected: the payload, and so the token, differs.
    assert!(r.mint("arena_final", 1).is_some());
}

#[test]
fn retire_expired_keeps_live_seats() {
    let mut r = SeatRegistry::new("secret");
    let live = r.mint("tank-3d", 600).unwrap();
    let dead = r.sign("tank-3d", 1);
    r.by_token.insert(
        dead.clone(),
        Seat {
            id: "dead".into(),
            room: "tank-3d".into(),
            token: dead.clone(),
            expires_at: 1,
        },
    );
    assert_eq!(r.outstanding(), 2);
    r.retire_expired();
    assert_eq!(r.outstanding(), 1);
    assert!(r.by_token.contains_key(&live.token));
    assert!(!r.by_token.contains_key(&dead));
}

#[test]
fn minting_is_bounded_and_says_so() {
    let mut r = SeatRegistry::with_seat_bound("secret", 4);
    for _ in 0..4 {
        assert!(r.mint("tank-3d", 300).is_some());
    }
    assert_eq!(r.outstanding(), 4);
    assert_eq!(r.seat_bound(), 4);
    assert!(r.mint("tank-3d", 300).is_none(), "the bound is enforced");
    assert_eq!(r.refused(), 1);
    assert_eq!(r.outstanding(), 4, "a refused mint stores nothing");
    assert_eq!(r.counts().0, 4, "and is not counted as a mint");
    // Freeing space lets minting resume: the bound is a ceiling, not a latch.
    let some = r.by_token.keys().next().cloned().unwrap();
    r.release(&some);
    assert!(r.mint("tank-3d", 300).is_some());
}

#[test]
fn the_bound_sweeps_expired_seats_instead_of_refusing() {
    let mut r = SeatRegistry::with_seat_bound("secret", 2);
    let old = r.mint("tank-3d", 300).unwrap();
    r.by_token.get_mut(&old.token).unwrap().expires_at = 1; // already dead
    assert!(r.mint("tank-3d", 300).is_some());
    assert_eq!(r.outstanding(), 2);
    // Full, but one entry is expired: mint sweeps, then succeeds.
    assert!(r.mint("tank-3d", 300).is_some());
    assert_eq!(r.refused(), 0);
    assert_eq!(r.outstanding(), 2);
}
