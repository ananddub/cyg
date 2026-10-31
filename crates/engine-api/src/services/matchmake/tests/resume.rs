use super::*;
use axum::http::StatusCode;

#[test]
fn a_replayed_reconnect_resumes_the_same_session() {
    let state = test_state(1000, 1000);
    let first = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
    let body = format!("{{\"reconnectionToken\":\"{}\"}}", first.seat_token);
    let again = reserve_ok(&state, "reconnect", "tank-3d", &body);
    let third = reserve_ok(&state, "reconnect", "tank-3d", &body);
    assert_eq!(
        again.session_id, first.session_id,
        "sessionId must be stable"
    );
    assert_eq!(third.session_id, first.session_id);
    assert_eq!(again.room_id, first.room_id);
    assert_eq!(
        again.seat_token, first.seat_token,
        "no token churn on resume"
    );
    assert_eq!(minted(&state), 1, "reconnect mints nothing");
}

#[test]
fn rejoin_resumes_from_the_reconnection_token_too() {
    let state = test_state(1000, 1000);
    let first = reserve_ok(&state, "create", "tank-3d", "{}");
    let body = format!("{{\"reconnectionToken\":\"{}\"}}", first.seat_token);
    let back = reserve_ok(&state, "rejoin", "tank-3d", &body);
    assert_eq!(back.session_id, first.session_id);
    assert_eq!(minted(&state), 1);
}

#[test]
fn a_reconnect_cannot_jump_rooms_via_the_url() {
    let state = test_state(1000, 1000);
    let first = reserve_ok(&state, "create", "lobby_a", "{}");
    let body = format!("{{\"reconnectionToken\":\"{}\"}}", first.seat_token);
    let hijack = reserve_ok(&state, "reconnect", "lobby_b", &body);
    assert_eq!(hijack.room_id, "lobby_a", "the URL did not move the seat");
    assert_eq!(hijack.name, "lobby_a");
}

#[test]
fn a_missing_or_malformed_token_is_a_bad_request() {
    let state = test_state(1000, 1000);
    for body in ["{}", "not json", "{\"reconnectionToken\":42}", ""] {
        let e = err(&state, ip(1), "reconnect", "tank-3d", body);
        assert_eq!(e.status, StatusCode::BAD_REQUEST, "body {body:?}");
    }
    assert_eq!(minted(&state), 0);
}

#[test]
fn an_expired_or_tampered_token_is_refused() {
    let state = test_state(1000, 1000);
    let seat = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
    let (payload, mac) = seat.seat_token.split_once('.').unwrap();
    let mut flipped: Vec<char> = mac.chars().collect();
    flipped[0] = if flipped[0] == 'A' { 'B' } else { 'A' };
    let tampered = format!("{payload}.{}", flipped.iter().collect::<String>());
    let expired = {
        let mut r = SeatRegistry::new(DEV_SEAT_SECRET);
        r.mint("tank-3d", 0).unwrap().token
    };
    for token in [
        tampered,
        expired,
        format!("{payload}AAAA.{mac}"),
        "a.b".to_string(),
    ] {
        let e = err(
            &state,
            ip(1),
            "reconnect",
            "tank-3d",
            &format!("{{\"reconnectionToken\":\"{token}\"}}"),
        );
        assert_eq!(e.status, StatusCode::UNAUTHORIZED, "token {token}");
    }
    assert_eq!(minted(&state), 1, "refusals mint nothing");
}
