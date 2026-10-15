use crate::infra::seats::*;

/// The payload has no nonce, so without the expiry bump two same-second mints
/// share a token, a sessionId, and one player identity on the game plane.
#[test]
fn two_clients_in_the_same_second_get_different_seats() {
    let mut r = SeatRegistry::new("secret");
    let a = r.mint("tank-3d", 300).unwrap();
    let b = r.mint("tank-3d", 300).unwrap();
    assert_ne!(a.token, b.token, "the token is the identity; it cannot repeat");
    assert_ne!(a.id, b.id);
    assert_eq!(r.outstanding(), 2);
    assert!(b.expires_at >= a.expires_at, "the bump only lengthens the TTL");
    assert_eq!(r.verify(&a.token).unwrap().id, a.id);
    assert_eq!(r.verify(&b.token).unwrap().id, b.id);
}

#[test]
fn mints_and_verifies_its_own_token() {
    let mut r = SeatRegistry::new("secret");
    let seat = r.mint("tank-3d", 300).unwrap();
    assert_eq!(r.verify(&seat.token).unwrap().room, "tank-3d");
}

/// `reconnect` must hand back the sessionId the SDK already put into its
/// WebSocket upgrade query, twice over if the page reloads twice.
#[test]
fn session_id_is_stable_across_repeated_verifies() {
    let mut r = SeatRegistry::new("secret");
    let seat = r.mint("tank-3d", 300).unwrap();
    let first = r.verify(&seat.token).unwrap();
    let second = r.verify(&seat.token).unwrap();
    assert_eq!(first, seat);
    assert_eq!(first.id, second.id);
    assert_eq!(first.token, second.token);
}

#[test]
fn mint_refuses_rooms_the_payload_layout_cannot_carry() {
    let mut r = SeatRegistry::new("secret");
    assert!(r.mint("", 300).is_none());
    assert!(r.mint("a:b", 300).is_none());
    assert!(r.mint("a.b", 300).is_none());
    assert_eq!(r.outstanding(), 0);
}
