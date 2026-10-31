use super::*;

#[test]
fn a_reservation_carries_every_field_the_sdk_needs() {
    let state = test_state(1000, 1000);
    let seat = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
    let json = seat.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    for field in [
        "name",
        "sessionId",
        "roomId",
        "processId",
        "reconnectionToken",
    ] {
        assert!(
            v.get(field).and_then(|f| f.as_str()).is_some(),
            "ISeatReservation is missing {field}: {json}"
        );
    }
    for field in [
        "seatToken",
        "gamePort",
        "controlPort",
        "expiresAt",
        "iceServers",
    ] {
        assert!(
            v.get(field).is_some(),
            "data-plane field {field} missing: {json}"
        );
    }
    assert_eq!(v["name"].as_str().unwrap(), "tank-3d");
    assert_eq!(v["roomId"].as_str().unwrap(), "tank-3d");
    assert_eq!(v["gamePort"].as_u64().unwrap(), u64::from(state.game.port));
    assert_eq!(v["controlPort"].as_u64().unwrap(), 8124);
    assert!(v["processId"].as_str().unwrap().starts_with("cyg_"));
    assert_eq!(v["seatToken"].as_str(), v["reconnectionToken"].as_str());
    assert!(v["expiresAt"].as_u64().unwrap() > now_unix());
    assert!(
        !v["sessionId"].as_str().unwrap().is_empty(),
        "sessionId identifies the seat"
    );
    let ice = v["iceServers"].as_str().expect("iceServers is a string");
    let parsed: serde_json::Value = serde_json::from_str(ice).unwrap();
    assert_eq!(
        parsed[0]["urls"][0].as_str().unwrap(),
        "stun:stun.l.google.com:19302"
    );
}

#[test]
fn every_mint_method_gets_a_seat() {
    let state = test_state(1000, 1000);
    for method in ["joinOrCreate", "join", "joinById", "create"] {
        let seat = reserve_ok(&state, method, "arena_final", "{}");
        assert_eq!(seat.room_id, "arena_final", "{method}");
        assert!(seat.seat_token.contains('.'), "{method} signed");
    }
    assert_eq!(minted(&state), 4);
}

#[test]
fn seat_json_is_parseable_and_nests_the_ice_blob_as_text() {
    let resp = SeatResponse {
        name: "tank-3d".into(),
        session_id: "s1".into(),
        room_id: "tank-3d".into(),
        process_id: "cyg_1".into(),
        reconnection_token: "tok".into(),
        seat_token: "tok".into(),
        game_port: 7777,
        control_port: 8090,
        udp_port: 7778,
        ice_servers: "[{\"urls\":[\"stun:stun.l.google.com:19302\"]}]".into(),
        expires_at: 1_800_000_000,
    };
    let outer: serde_json::Value = serde_json::from_str(&resp.to_json()).unwrap();
    assert_eq!(outer["sessionId"].as_str().unwrap(), "s1");
    assert_eq!(outer["gamePort"].as_u64().unwrap(), 7777);
    assert_eq!(outer["udpPort"].as_u64().unwrap(), 7778);
    let ice = outer["iceServers"].as_str().unwrap();
    let inner: serde_json::Value = serde_json::from_str(ice).unwrap();
    assert_eq!(
        inner[0]["urls"][0].as_str().unwrap(),
        "stun:stun.l.google.com:19302"
    );
}

#[test]
fn a_quote_in_a_seat_field_cannot_break_the_envelope() {
    let resp = SeatResponse {
        name: "t\"ank".into(),
        session_id: "s1".into(),
        room_id: "r".into(),
        process_id: "p".into(),
        reconnection_token: "a\\b".into(),
        seat_token: "t".into(),
        game_port: 1,
        control_port: 2,
        udp_port: 7778,
        ice_servers: "[]".into(),
        expires_at: 3,
    };
    let outer: serde_json::Value = serde_json::from_str(&resp.to_json()).unwrap();
    assert_eq!(outer["name"].as_str().unwrap(), "t\"ank");
    assert_eq!(outer["reconnectionToken"].as_str().unwrap(), "a\\b");
}
