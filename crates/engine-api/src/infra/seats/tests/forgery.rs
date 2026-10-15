use crate::infra::seats::*;
use engine_config::DEV_SEAT_SECRET;

#[test]
fn rejects_a_tampered_payload() {
    let mut r = SeatRegistry::new("secret");
    let seat = r.mint("tank-3d", 300).unwrap();
    let (_, mac) = seat.token.split_once('.').unwrap();
    // Same MAC, different room: the payload is not covered by it any more.
    let forged = format!("{}.{mac}", b64url(b"other-room:9999999999"));
    assert!(r.verify(&forged).is_none());
}

#[test]
fn rejects_an_expired_seat_without_trusting_the_payload() {
    let mut r = SeatRegistry::new("secret");
    let token = r.sign("tank-3d", 1);
    assert!(r.verify(&token).is_none());
}

#[test]
fn rejects_a_token_signed_with_another_secret() {
    let other = SeatRegistry::new(DEV_SEAT_SECRET).sign("tank-3d", 4_000_000_000);
    let mut r = SeatRegistry::new("a-different-secret");
    assert!(r.verify(&other).is_none());
}

#[test]
fn every_refusal_is_counted() {
    let mut r = SeatRegistry::new("secret");
    let good = r.mint("tank-3d", 300).unwrap();
    let bad = [
        "",
        "no-dot",
        "a.b",
        "@@@.###",
        "AA==.AA==",
        &format!("{}.", good.token),
    ];
    for token in bad {
        assert!(r.verify(token).is_none(), "must reject {token:?}");
    }
    let (minted, rejected) = r.counts();
    assert_eq!(minted, 1);
    assert_eq!(rejected, bad.len() as u64, "one bump per refused token");
}

/// The MAC, not the map, is the authority (see the module docs).
#[test]
fn a_released_seat_is_gone_from_the_map_but_still_authenticates() {
    let mut r = SeatRegistry::new("secret");
    let seat = r.mint("tank-3d", 300).unwrap();
    assert_eq!(r.outstanding(), 1);
    assert_eq!(r.release(&seat.token).unwrap().id, seat.id);
    assert_eq!(r.outstanding(), 0);
    assert_eq!(r.verify(&seat.token).unwrap().room, "tank-3d");
}
