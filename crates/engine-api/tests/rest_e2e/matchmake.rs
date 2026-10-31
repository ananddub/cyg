use super::*;

pub async fn matchmake(client: &Client, base: &str, port: u16) -> String {
    eprintln!("e2e: matchmake");
    let seat = json_body(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/e2e-room"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "matchmake joinOrCreate",
    )
    .await;
    assert_eq!(seat["name"], "e2e-room");
    assert_eq!(seat["roomId"], "e2e-room");
    assert_eq!(seat["controlPort"], port);
    assert_eq!(seat["gamePort"], 7777);
    let session_id = seat["sessionId"].as_str().expect("sessionId").to_owned();
    assert!(!session_id.is_empty());
    let token = seat["reconnectionToken"]
        .as_str()
        .expect("reconnectionToken")
        .to_owned();
    assert_eq!(seat["seatToken"], token);
    let ice: Value = serde_json::from_str(seat["iceServers"].as_str().unwrap())
        .expect("iceServers is JSON text");
    assert!(ice.is_array());
    assert!(seat["udpPort"].as_u64().is_some_and(|p| p > 0));

    let other = json_body(
        client
            .post(format!("{base}/matchmake/join/create"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "matchmake via /matchmake",
    )
    .await;
    assert_eq!(other["name"], "create");
    assert_ne!(other["sessionId"], session_id);

    let resumed = json_body(
        client
            .post(format!("{base}/api/matchmake/reconnect/e2e-room"))
            .json(&json!({ "reconnectionToken": token }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reconnect",
    )
    .await;
    assert_eq!(resumed["sessionId"], session_id);
    assert_eq!(resumed["reconnectionToken"], token);

    let unknown = refusal(
        client
            .post(format!("{base}/api/matchmake/reconnect/e2e-room"))
            .json(&json!({ "reconnectionToken": "not-a-seat" }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "reconnect with a bad token",
    )
    .await;
    assert!(unknown.contains("expired or unknown"), "{unknown}");

    expect(
        client
            .post(format!("{base}/api/matchmake/teleport/e2e-room"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "unknown matchmake method",
    )
    .await;
    expect(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/bad%20room"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "invalid room name",
    )
    .await;
    expect(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/e2e-room"))
            .body("x".repeat(4097))
            .send()
            .await
            .unwrap(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "oversized matchmake body",
    )
    .await;

    let status = json_body(
        client.get(format!("{base}/status")).send().await.unwrap(),
        StatusCode::OK,
        "GET /status after matchmake",
    )
    .await;
    assert_eq!(status["seats"]["minted"], 2);
    assert!(status["seats"]["outstanding"].as_u64().unwrap() >= 2);

    token
}
