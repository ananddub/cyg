use super::*;

pub async fn matchmake_plane(env: &Harness) -> String {
    eprintln!("gap: matchmake");
    let room = format!("gap_room_{}", letters_suffix());

    let mut seat_token = String::new();
    for method in ["joinOrCreate", "join", "joinById", "create"] {
        let body = json_body(
            env.client
                .post(format!("{}/matchmake/{method}/{room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            &format!("POST /matchmake/{method}/{room}"),
        )
        .await;
        assert_eq!(body["name"], room, "{method}: the room name echoes back");
        assert_eq!(body["roomId"], room);
        assert!(
            body["sessionId"].as_str().is_some_and(|id| !id.is_empty()),
            "{method}: a session id: {body}"
        );
        assert!(
            body["processId"]
                .as_str()
                .unwrap_or_default()
                .starts_with("cyg_"),
            "{method}: the host process is named for the plane: {body}"
        );
        assert_eq!(
            body["gamePort"].as_i64(),
            Some(7777),
            "{method}: the game port is the configured one, not the API port"
        );
        assert_eq!(
            body["controlPort"].as_i64(),
            Some(env.port as i64),
            "{method}: the control port is the socket the client just dialled"
        );
        assert!(
            body["udpPort"].as_i64().unwrap_or_default() > 0,
            "{method}: a udp port is offered: {body}"
        );
        assert_eq!(
            body["iceServers"].as_str(),
            Some("[]"),
            "{method}: iceServers travels as a JSON string, not an array: {body}"
        );
        let expires = body["expiresAt"].as_u64().unwrap_or_default();
        assert!(
            expires > now_secs(),
            "{method}: the seat expires in the future, not in the past"
        );
        let token = body["seatToken"].as_str().expect("seat token").to_owned();
        assert_eq!(
            body["reconnectionToken"].as_str(),
            Some(token.as_str()),
            "{method}: the seat token and the reconnection token are the one HMAC"
        );
        seat_token = token;
    }

    json_body(
        env.client
            .post(format!("{}/api/matchmake/create/{room}", env.base))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /api/matchmake/create/{room}",
    )
    .await;

    for method in ["rejoin", "reconnect"] {
        let resumed = json_body(
            env.client
                .post(format!("{}/matchmake/{method}/{room}", env.base))
                .json(&json!({ "reconnectionToken": seat_token }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            &format!("POST /matchmake/{method} with a live token"),
        )
        .await;
        assert_eq!(resumed["name"], room);
        assert_eq!(resumed["seatToken"].as_str(), Some(seat_token.as_str()));
    }

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/matchmake/speculate/{room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "an unknown matchmake method",
        )
        .await,
        "no room handler for method 'speculate'"
    );
    for (label, bad_room) in [
        ("a room with a bang", "bad!room"),
        ("a room with a space", "bad%20room"),
        ("a room longer than 64 characters", &"r".repeat(65)),
    ] {
        let body = json_body(
            env.client
                .post(format!("{}/matchmake/create/{bad_room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            label,
        )
        .await;
        assert_eq!(
            body["error"]["message"],
            "invalid room name: use 1-64 letters, digits, '-' or '_'"
        );
        assert_eq!(body["error"]["code"], 3, "400 maps to INVALID_ARGUMENT");
    }
    let resp = env
        .client
        .post(format!("{}/matchmake/create/{room}", env.base))
        .body("x".repeat(4097))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "a matchmake body over 4096 bytes is refused"
    );
    assert_eq!(
        resp.text().await.unwrap_or_default(),
        "",
        "the ceiling fires before any parsing, so there is nothing to echo"
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/matchmake/reconnect/{room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "reconnect with no reconnectionToken",
        )
        .await,
        "missing reconnectionToken"
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/matchmake/reconnect/{room}", env.base))
                .json(&json!({ "reconnectionToken": "not.a.signed-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "reconnect with a forged token",
        )
        .await,
        "reconnection token expired or unknown"
    );

    seat_token
}
