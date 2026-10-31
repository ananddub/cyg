use super::*;

pub async fn telemetry(client: &Client, base: &str, port: u16) {
    eprintln!("e2e: telemetry");
    let health = json_body(
        client
            .get(format!("{base}/auto/health"))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /auto/health",
    )
    .await;
    assert_eq!(health["status"], "ok");
    assert_eq!(health["routing"], "auto");

    let status = json_body(
        client.get(format!("{base}/status")).send().await.unwrap(),
        StatusCode::OK,
        "GET /status",
    )
    .await;
    assert_eq!(status["plane"]["controlPort"], port);
    assert_eq!(status["plane"]["game"]["name"], GAME_NAME);
    assert_eq!(status["plane"]["game"]["port"], 7777);
    assert_eq!(status["seats"]["minted"], 0);
    assert_eq!(status["seats"]["outstanding"], 0);
    assert_eq!(status["rateLimit"]["rps"], 1000);
    assert!(
        status["sim"].is_null(),
        "no sim is attached in this harness"
    );

    let rooms = json_body(
        client
            .get(format!("{base}/api/rooms"))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/rooms",
    )
    .await;
    let rooms = rooms.as_array().expect("/api/rooms is a list");
    assert_eq!(rooms.len(), 1);
    let room = &rooms[0];
    assert_eq!(room["name"], GAME_NAME);
    assert_eq!(room["maxClients"], 16);
    assert_eq!(room["controlPort"], port);
    assert_eq!(room["port"], 7777);
    assert_eq!(room["hasSimSample"], false);
    assert!(
        room["ageMs"].is_null(),
        "no sample means the age is unknown"
    );
}
