use super::*;

pub async fn games(client: &Client, base: &str, port: u16, console: &str) -> (String, String) {
    eprintln!("e2e: games and unity settings");
    let suffix = letters_suffix();
    let unity_name = format!("E2E Unity {suffix}");
    let unity = json_body(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": unity_name, "engine_type": "unity" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a Unity game",
    )
    .await;
    assert_eq!(unity["engine_type"], "unity");
    assert_eq!(unity["backend_mode"], "relay");
    assert_eq!(unity["status"], "active");
    assert!(unity["game_key"].as_str().unwrap().starts_with("gm_"));
    assert!(
        unity["client_key"]
            .as_str()
            .unwrap()
            .starts_with("cyg_pub_")
    );
    assert!(
        unity.get("api_key").is_none(),
        "the secret belongs to the detail view only"
    );
    assert_eq!(unity["config"]["tick_rate"], 128);
    assert_eq!(unity["config"]["max_clients"], 16);
    let unity_id = unity["id"].as_str().expect("game id").to_owned();

    let wasm_name = format!("E2E Custom {suffix}");
    let wasm_game = json_body(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({
                "name": wasm_name,
                "engine_type": "custom",
                "tick_rate": 60,
                "max_clients": 32,
                "persistent_rooms": true
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a custom (WASM) game",
    )
    .await;
    assert_eq!(wasm_game["engine_type"], "custom");
    assert_eq!(wasm_game["backend_mode"], "wasm");
    assert_eq!(wasm_game["config"]["tick_rate"], 60);
    assert_eq!(wasm_game["config"]["max_clients"], 32);
    assert_eq!(wasm_game["config"]["persistent_rooms"], true);
    let wasm_id = wasm_game["id"].as_str().expect("game id").to_owned();

    refusal(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": "   " }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "create with a blank name",
    )
    .await;
    refusal(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": "Frostbite", "engine_type": "frostbite" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "create with an unknown engine",
    )
    .await;
    refusal(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": "Bad Config", "config": [1, 2] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "create with a non-object config",
    )
    .await;

    let listing = json_body(
        client
            .get(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list games",
    )
    .await;
    assert_eq!(listing["total"], 3);
    assert!(
        listing["games"]
            .as_array()
            .unwrap()
            .iter()
            .any(|game| game["id"] == unity_id)
    );

    let detail = json_body(
        client
            .get(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "game detail",
    )
    .await;
    let api_key = detail["api_key"].as_str().expect("api_key").to_owned();
    assert!(api_key.starts_with("cyg_sec_"));
    assert_eq!(detail["module_count"], 0);
    assert!(detail["active_module_version"].is_null());

    let renamed = format!("E2E Unity Renamed {suffix}");
    let updated = json_body(
        client
            .put(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .json(&json!({ "name": renamed, "status": "suspended" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "patch a game",
    )
    .await;
    assert_eq!(updated["name"], renamed);
    assert_eq!(updated["status"], "suspended");
    assert_eq!(updated["backend_mode"], "relay");

    refusal(
        client
            .put(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "empty game patch",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/console/games/{}", Uuid::new_v4()))
            .bearer_auth(console)
            .json(&json!({ "name": "ghost" }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "patch an unknown game",
    )
    .await;

    let settings = json_body(
        client
            .get(format!(
                "{base}/v2/console/games/{unity_id}/unity-settings?format=json"
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unity settings JSON",
    )
    .await;
    assert_eq!(settings["gameKey"], unity["game_key"]);
    assert_eq!(settings["clientKey"], unity["client_key"]);
    assert!(settings.get("apiKey").is_none());
    assert_eq!(
        settings["serverHttpUrl"],
        format!("http://127.0.0.1:{port}")
    );
    assert_eq!(settings["serverWsPath"], "/ws");
    assert_eq!(settings["defaultRoom"], "cyg_room");
    assert_eq!(settings["tickRate"], 128);
    assert_eq!(settings["maxPlayers"], 16);
    assert_eq!(settings["persistentRooms"], false);
    assert_eq!(settings["backendMode"], "relay");

    let revealed = json_body(
        client
            .get(format!(
                "{base}/v2/console/games/{unity_id}/unity-settings?format=json&reveal=true"
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unity settings JSON with reveal",
    )
    .await;
    assert_eq!(revealed["apiKey"], api_key);

    let asset_resp = client
        .get(format!("{base}/v2/console/games/{unity_id}/unity-settings"))
        .bearer_auth(console)
        .send()
        .await
        .unwrap();
    assert_eq!(asset_resp.status(), StatusCode::OK);
    assert_eq!(
        asset_resp.headers()["content-type"],
        "application/octet-stream"
    );
    assert!(
        asset_resp.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("CYGServerSettings.asset")
    );
    let asset = asset_resp.text().await.unwrap();
    assert!(asset.contains("m_Name: CYGServerSettings"), "{asset}");
    assert!(asset.contains(&format!("serverHttpUrl: http://127.0.0.1:{port}")));
    assert!(asset.contains("serverWsPath: /ws"));
    assert!(asset.contains("roomName: cyg_room"));
    assert!(asset.contains("maxPlayers: 16"));
    assert!(asset.contains("protocolVersion: 1"));
    assert!(asset.contains("matchmakeMethod: joinOrCreate"));

    refusal(
        client
            .get(format!(
                "{base}/v2/console/games/{unity_id}/unity-settings?format=xml"
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "unknown unity-settings format",
    )
    .await;

    let archived = json_body(
        client
            .delete(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "archive a game",
    )
    .await;
    assert_eq!(archived["archived"], true);
    let after = json_body(
        client
            .get(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read an archived game",
    )
    .await;
    assert_eq!(after["status"], "archived");
    refusal(
        client
            .delete(format!("{base}/v2/console/games/{}", Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "archive an unknown game",
    )
    .await;

    (unity_id, wasm_id)
}
