use super::server::{TestServer, header};
use super::{MAX_BODY_BYTES, test_state};

#[test]
fn both_paths_serve_a_seat_over_http() {
    for path in ["/matchmake", "/api/matchmake"] {
        let server = TestServer::start(test_state(1000, 1000));
        let (status, headers, body) = server.post(&format!("{path}/joinOrCreate/tank-3d"), "{}");
        assert_eq!(status, 200, "{path}: {body}");
        assert_eq!(header(&headers, "content-type"), Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(&body).expect("seat is JSON");
        assert_eq!(v["roomId"].as_str().unwrap(), "tank-3d");
        assert!(!v["sessionId"].as_str().unwrap().is_empty());
        assert!(v["seatToken"].as_str().unwrap().contains('.'));
        assert!(
            v["reconnectionToken"]
                .as_str()
                .unwrap()
                .starts_with("dGFuay0z")
        );
    }
}

#[test]
fn http_reconnect_hands_back_the_same_session_id() {
    let server = TestServer::start(test_state(1000, 1000));
    let (_, _, body) = server.post("/matchmake/joinOrCreate/tank-3d", "{}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let token = v["reconnectionToken"].as_str().unwrap().to_string();
    let first = v["sessionId"].as_str().unwrap().to_string();
    let (status, _, again) = server.post(
        "/matchmake/reconnect/tank-3d",
        &format!("{{\"reconnectionToken\":\"{token}\"}}"),
    );
    assert_eq!(status, 200, "reconnect: {again}");
    let v2: serde_json::Value = serde_json::from_str(&again).unwrap();
    assert_eq!(v2["sessionId"].as_str().unwrap(), first.as_str());
    assert_eq!(v2["seatToken"].as_str().unwrap(), token.as_str());
}

#[test]
fn http_over_the_burst_is_a_429_with_retry_after() {
    let server = TestServer::start(test_state(1, 2));
    for _ in 0..2 {
        let (status, _, body) = server.post("/matchmake/joinOrCreate/tank-3d", "{}");
        assert_eq!(status, 200, "in-burst request: {body}");
    }
    let (status, headers, body) = server.post("/matchmake/joinOrCreate/tank-3d", "{}");
    assert_eq!(status, 429, "body: {body}");
    let retry = header(&headers, "retry-after").expect("Retry-After is sent");
    let secs: u32 = retry.parse().expect("delta seconds");
    assert!((1..=2).contains(&secs), "honest wait, got {secs}");
    let v: serde_json::Value = serde_json::from_str(&body).expect("error body is JSON");
    assert_eq!(v["error"]["code"], 8, "429 maps to RESOURCE_EXHAUSTED");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("rate limit")
    );
}

#[test]
fn http_refuses_a_hostile_room_name_and_the_body_stays_json() {
    let server = TestServer::start(test_state(1000, 1000));
    for room in ["evil%22name", "evil%2Fname", "evil%0Aname"] {
        let (status, headers, body) = server.post(&format!("/matchmake/joinOrCreate/{room}"), "{}");
        assert_eq!(status, 400, "{room}: {body}");
        assert_eq!(header(&headers, "content-type"), Some("application/json"));
        serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or_else(|e| panic!("{room} body not JSON: {e}: {body:?}"));
    }
}

#[test]
fn http_rejects_a_body_over_the_limit() {
    let server = TestServer::start(test_state(1000, 1000));
    let big = "x".repeat(MAX_BODY_BYTES + 1024);
    let (status, _, _) = server.post(
        "/matchmake/reconnect/tank-3d",
        &format!("{{\"reconnectionToken\":\"{big}\"}}"),
    );
    assert_eq!(status, 413, "the ceiling is enforced before parsing");
}

#[test]
fn http_unknown_method_is_404_and_a_control_char_does_not_break_the_body() {
    let server = TestServer::start(test_state(1000, 1000));
    let (status, _, body) = server.post("/matchmake/spectate/tank-3d", "{}");
    assert_eq!(status, 404, "{body}");
    let (status, _, body) = server.post("/matchmake/spec%01tate/tank-3d", "{}");
    assert_eq!(status, 404);
    serde_json::from_str::<serde_json::Value>(&body)
        .unwrap_or_else(|e| panic!("error body must stay JSON, got {body:?}: {e}"));
}
