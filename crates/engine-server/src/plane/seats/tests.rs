use super::registry::SeatRegistry;
use super::token::b64url_decode;
use super::token::b64url_encode;

fn reg() -> SeatRegistry {
    SeatRegistry::new("test-secret")
}

#[test]
fn mint_verify_roundtrip() {
    let token = reg().mint("tank-3d", 300);
    assert!(token.contains('.'), "token shape is payload.sig: {token}");
    let grant = reg().verify(&token).expect("valid token must verify");
    assert_eq!(grant.room, "tank-3d");
}

#[test]
fn b64url_codec_roundtrip() {
    for sample in [
        b"a".as_slice(),
        b"ab",
        b"abc",
        b"abcd",
        b"tank-3d:1700000000",
        b"\xff\xfe\x01\x80",
    ] {
        let enc = b64url_encode(sample);
        assert!(!enc.contains('='));
        assert_eq!(b64url_decode(&enc).as_deref(), Some(sample));
    }
}

#[test]
fn expired_rejected() {
    let token = reg().mint("tank-3d", 0);
    assert!(reg().verify(&token).is_none());
}

#[test]
fn tampered_mac_rejected() {
    let token = reg().mint("tank-3d", 300);
    let (payload, sig) = token.split_once('.').unwrap();
    let mut bytes: Vec<char> = sig.chars().collect();
    bytes[0] = if bytes[0] == 'A' { 'B' } else { 'A' };
    let tampered = format!("{payload}.{}", bytes.iter().collect::<String>());
    assert!(reg().verify(&tampered).is_none());

    let other = reg().mint("other-room", 300);
    let (_, other_sig) = other.split_once('.').unwrap();
    assert!(reg().verify(&format!("{payload}.{other_sig}")).is_none());
}

#[test]
fn wrong_secret_rejected() {
    let token = reg().mint("tank-3d", 300);
    assert!(SeatRegistry::new("other-secret").verify(&token).is_none());
}

#[test]
fn a_seat_minted_with_the_retired_secret_verifies_until_the_window_closes() {
    let token = SeatRegistry::with_previous("old-secret", "").mint("tank-3d", 300);
    assert_eq!(
        SeatRegistry::with_previous("new-secret", "old-secret")
            .verify(&token)
            .expect("the retired secret is still trusted while seats live")
            .room,
        "tank-3d"
    );
    assert!(
        SeatRegistry::with_previous("new-secret", "")
            .verify(&token)
            .is_none(),
        "once the retired secret is dropped the seat must stop verifying"
    );
}

#[test]
fn garbage_rejected_no_panic() {
    for bad in [
        "",
        ".",
        "..",
        "no-dot",
        "a.b",
        "A.B",
        "@@@.###",
        "AA==.AA==",
        "QQ.a",
        "QQQ.aaa",
        &format!("{}.AAAAAAAA", b64url_encode(b"tank-3d:notanumber")),
        &format!("{}.AAAAAAAA", b64url_encode(b":19999999999")),
    ] {
        assert!(reg().verify(bad).is_none(), "must reject: {bad:?}");
    }
}
