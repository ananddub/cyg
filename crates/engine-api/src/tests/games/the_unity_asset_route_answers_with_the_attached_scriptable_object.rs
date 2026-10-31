use super::*;

#[tokio::test]
pub(crate) async fn the_unity_asset_route_answers_with_the_attached_scriptable_object() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let auth = admin_token();
    let game = db::create_engine(&state, "unity-asset", Some("unity")).await;

    let (status, headers, body) = fetch_raw(
        &pool,
        "GET",
        &format!("/v2/console/games/{}/unity-settings", game.id),
        &auth,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        headers
            .iter()
            .find(|(name, _)| name == "content-disposition")
            .map(|(_, value)| value.as_str()),
        Some("attachment; filename=\"CYGServerSettings.asset\""),
        "the asset has to arrive as a file with the name the SDK expects: {headers:?}"
    );

    assert!(body.starts_with("%YAML 1.1"), "{body}");
    for marker in [
        "%TAG !u! tag:unity3d.com,2011:",
        "--- !u!114 &11400000",
        "MonoBehaviour:",
        "m_Script:",
        "m_Name: CYGServerSettings",
    ] {
        assert!(
            body.contains(marker),
            "an asset carries `{marker}`:\n{body}"
        );
    }

    let emitted = {
        let mut keys: Vec<String> = body
            .lines()
            .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
            .filter_map(|line| line[2..].split_once(':'))
            .map(|(key, _)| key.trim().to_owned())
            .filter(|key| !key.is_empty() && !key.contains('_'))
            .collect();
        keys.sort();
        keys
    };
    let mut declared = sdk_serialized_fields();
    declared.sort();
    assert_eq!(
        emitted, declared,
        "the asset writes exactly the SDK's fields:\n{body}"
    );

    assert_eq!(yaml_field(&body, "roomName"), "cyg_room");
    assert_eq!(yaml_field(&body, "serverWsPath"), "/ws");
    assert_eq!(yaml_field(&body, "matchmakeMethod"), "joinOrCreate");
    assert_eq!(
        yaml_field(&body, "maxPlayers"),
        game.config["max_clients"].to_string(),
        "the advertised capacity is the config's own: {body}"
    );
    let protocol: u32 = yaml_field(&body, "protocolVersion")
        .parse()
        .expect("protocolVersion is a number in the emitted asset");
    assert!(
        protocol > 0 && protocol <= u32::from(u16::MAX),
        "{protocol}"
    );
    assert!(
        yaml_field(&body, "serverHttpUrl").starts_with("http://"),
        "an asset that points at a non-http origin cannot be opened by the SDK's own \
         uri parser: {body}"
    );

    for secret in [
        game.game_key.as_str(),
        game.client_key.as_str(),
        GAME_KEY_PREFIX,
        CLIENT_KEY_PREFIX,
        API_KEY_PREFIX,
    ] {
        assert!(
            !body.contains(secret),
            "the asset must not carry `{secret}`:\n{body}"
        );
    }

    db::forget(&pool, game.id).await;
}
#[tokio::test]
pub(crate) async fn the_unity_settings_json_carries_the_secret_only_when_revealed() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let auth = admin_token();
    let game = db::create_engine(&state, "unity-json", Some("unity")).await;
    let path = format!("/v2/console/games/{}/unity-settings?format=json", game.id);

    let (status, _headers, body) = fetch_raw(&pool, "GET", &path, &auth).await;
    assert_eq!(status, 200, "{body}");
    let connection: Value = serde_json::from_str(&body).expect("the json format answers in json");
    assert_eq!(connection["gameKey"], json!(game.game_key));
    assert_eq!(connection["clientKey"], json!(game.client_key));
    assert_eq!(connection["backendMode"], json!("relay"));
    assert_eq!(connection["defaultRoom"], json!("cyg_room"));
    assert_eq!(connection["serverWsPath"], json!("/ws"));
    assert_eq!(
        connection["tickRate"], game.config["tick_rate"],
        "the connection reports the game's own tuning: {connection}"
    );
    assert!(
        connection.get("apiKey").is_none(),
        "a settings download is not a secret handout: {connection}"
    );

    let (status, _headers, revealed) =
        fetch_raw(&pool, "GET", &format!("{path}&reveal=true"), &auth).await;
    assert_eq!(status, 200, "{revealed}");
    let value: Value = serde_json::from_str(&revealed).expect("the revealed answer is json");
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the detail view reads");
    assert_eq!(value["apiKey"], json!(detail.api_key));
    assert_eq!(value["clientKey"], json!(game.client_key));
    assert_eq!(
        value["gameKey"],
        json!(game.game_key),
        "revealing changes the secret's presence and nothing else"
    );

    let roomy = db::create_from_value(
        &state,
        json!({"name": db::fixture_name("unity-clamp"), "engine_type": "web",
               "config": {"tick_rate": 20, "max_clients": 1000, "persistent_rooms": true,
                          "auto_close_seconds": 7}}),
    )
    .await;
    let connection =
        GamesService::unity_settings(&state, roomy.id, false, &axum::http::HeaderMap::new())
            .await
            .expect("an authoritative game has a connection document too");
    let wire = serde_json::to_value(&connection).expect("the connection serializes");
    assert_eq!(wire["maxPlayers"], json!(255), "{wire}");
    assert_eq!(wire["tickRate"], json!(20));
    assert_eq!(wire["persistentRooms"], json!(true));
    assert_eq!(
        wire["backendMode"],
        json!("wasm"),
        "and a wasm game is honest about being one"
    );

    db::forget(&pool, game.id).await;
    db::forget(&pool, roomy.id).await;
}
