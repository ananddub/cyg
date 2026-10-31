use super::*;

pub async fn dashboard_api(env: &Harness, minted: u64) {
    eprintln!("gap: dashboard api reads the live registry");

    let identity = json_body(
        env.client
            .get(format!("{}/api/dashboard/identity", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/identity",
    )
    .await;
    assert_eq!(identity["game_name"], GAME_NAME, "{identity}");
    assert!(
        identity["game_id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .is_some(),
        "game_id names the auto-registered game: {identity}"
    );

    let overview = json_body(
        env.client
            .get(format!("{}/api/dashboard/overview", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/overview",
    )
    .await;
    assert_eq!(overview["server"]["name"], GAME_NAME);
    assert_eq!(overview["server"]["port"], 7777);
    assert_eq!(overview["server"]["controlPort"], env.port);
    assert_eq!(overview["rooms"], 1, "this process hosts its one room");
    assert_eq!(
        overview["seats"]["minted"].as_u64().unwrap_or_default() >= minted,
        true,
        "the seat registry counts the mints this suite made: {overview}"
    );
    assert_eq!(overview["seats"]["ttlSecs"], 300, "the harness default TTL");
    assert_eq!(overview["rateLimit"]["rps"], 1000);

    let rooms = json_body(
        env.client
            .get(format!("{}/api/dashboard/rooms", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/rooms",
    )
    .await;
    let rows = rooms.as_array().expect("an array of rooms").clone();
    assert_eq!(rows.len(), 1, "{rooms}");
    assert_eq!(rows[0]["name"], GAME_NAME);
    assert_eq!(
        rows[0]["status"], "empty",
        "no client joined through this plane"
    );
    assert_eq!(
        rows[0]["signalingUrl"],
        format!("ws://127.0.0.1:{}/ws/signal", env.port)
    );

    let workers = json_body(
        env.client
            .get(format!("{}/api/dashboard/workers", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/workers",
    )
    .await;
    let names: Vec<&str> = workers
        .as_array()
        .expect("an array of workers")
        .iter()
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert!(
        names.contains(&"sim") && names.contains(&"control"),
        "{workers}"
    );

    let clients = json_body(
        env.client
            .get(format!("{}/api/dashboard/clients", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/clients",
    )
    .await;
    assert_eq!(
        clients.as_array().map(Vec::len),
        Some(0),
        "no player sessions on this process: {clients}"
    );

    let ticks = json_body(
        env.client
            .get(format!("{}/api/dashboard/ticks", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/ticks",
    )
    .await;
    assert!(ticks["points"].is_array(), "a tick series: {ticks}");

    let stranger = Uuid::new_v4().to_string();
    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/api/dashboard/overview?game_id={stranger}",
                    env.base
                ))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a scoped overview for an unregistered game",
        )
        .await,
        "Game not found."
    );
}
