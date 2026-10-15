use super::*;
use axum::http::StatusCode;
use axum::response::IntoResponse;

#[test]
fn the_n_plus_oneth_request_from_one_ip_is_denied_but_another_ip_passes() {
    let state = test_state(1, 3); // 1 token/s, burst of 3
    for i in 0..3 {
        assert!(
            reserve(&state, ip(1), "joinOrCreate", "tank-3d", "{}").is_ok(),
            "request {i} is inside the burst"
        );
    }
    let e = err(&state, ip(1), "joinOrCreate", "tank-3d", "{}");
    assert_eq!(e.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(e.retry_after.unwrap() >= 1, "the denial says when to return");
    // A different source is untouched by that bucket.
    assert!(reserve(&state, ip(2), "joinOrCreate", "tank-3d", "{}").is_ok());
}

/// The anti-spoofing/anti-cost property: after the bucket says no, the
/// registry must not have done any work.
#[test]
fn a_denied_request_costs_no_hmac_and_no_map_entry() {
    let state = test_state(1, 1);
    assert!(reserve(&state, ip(1), "joinOrCreate", "tank-3d", "{}").is_ok());
    let before = minted(&state);
    let denied = state.limiter.lock().unwrap().counts().1;
    for _ in 0..200 {
        assert!(reserve(&state, ip(1), "joinOrCreate", "tank-3d", "{}").is_err());
    }
    assert_eq!(minted(&state), before, "nothing was minted while denied");
    assert_eq!(state.limiter.lock().unwrap().counts().1, denied + 200);
}

/// Cost ordering holds on the resume path too: an over-budget client does not
/// get its token checked at all.
#[test]
fn a_denied_reconnect_is_refused_by_the_bucket_not_by_the_token() {
    let state = test_state(1, 1);
    let seat = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
    let body = format!("{{\"reconnectionToken\":\"{}\"}}", seat.seat_token);
    let e = err(&state, ip(1), "reconnect", "tank-3d", &body);
    assert_eq!(e.status, StatusCode::TOO_MANY_REQUESTS);
    // The same token from a fresh source still resumes.
    let back = ok(&state, ip(9), "reconnect", "tank-3d", &body);
    assert_eq!(back.session_id, seat.session_id);
}

#[test]
fn the_retry_after_header_reaches_the_client() {
    let resp = ApiError::rate_limited(7).into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
        Some("7")
    );
}
