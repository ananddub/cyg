use super::*;

#[tokio::test]
pub(crate) async fn create_game_mints_the_prefixed_keys_and_defaults_the_config() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "keys").await;
    assert!(
        game.game_key.starts_with(GAME_KEY_PREFIX),
        "a tenant key is minted with the `{GAME_KEY_PREFIX}` prefix: {}",
        game.game_key
    );
    assert!(
        game.game_key.len() > GAME_KEY_PREFIX.len(),
        "and it is a prefix of something, not the whole key"
    );
    assert_eq!(game.status, "active", "a new game is live");
    assert_eq!(
        game.engine_type, "unity",
        "an unspecified engine is the schema's own default"
    );
    assert_eq!(
        game.backend_mode, "relay",
        "a Unity game is served by the C# relay, and that is not the caller's to decide"
    );
    assert_eq!(game.config["tick_rate"], json!(128), "{}", game.config);
    assert_eq!(game.config["max_clients"], json!(16));
    assert_eq!(game.config["persistent_rooms"], json!(false));
    assert_eq!(game.config["auto_close_seconds"], json!(30));
    for retired in ["physics", "lag_compensation", "allow_storage"] {
        assert!(
            game.config.get(retired).is_none(),
            "the retired `{retired}` key must not be in a fresh config: {}",
            game.config
        );
    }

    assert!(
        game.client_key.starts_with(CLIENT_KEY_PREFIX),
        "the publishable client key is minted with the `{CLIENT_KEY_PREFIX}` prefix: {}",
        game.client_key
    );
    assert_eq!(
        serde_json::to_value(&game)
            .expect("a create reply serializes")
            .get("api_key"),
        None,
        "and no write reply carries the secret"
    );

    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the game just created is readable");
    assert!(
        detail.api_key.starts_with(API_KEY_PREFIX),
        "the secret carries the `{API_KEY_PREFIX}` prefix"
    );
    assert!(
        detail.api_key.len() <= 128,
        "and fits its VARCHAR(128) column: {}",
        detail.api_key.len()
    );
    assert_eq!(detail.game_key, game.game_key);
    assert_eq!(detail.client_key, game.client_key);
    assert_eq!(detail.backend_mode, game.backend_mode);

    assert_ne!(detail.api_key, detail.client_key);
    assert_ne!(detail.client_key, detail.game_key);
    assert!(detail.client_key.len() > CLIENT_KEY_PREFIX.len());

    let body = CreateGameRequest {
        name: db::fixture_name("explicit"),
        engine_type: Some("GODOT".to_owned()),
        config: Some(json!({"tick_rate": 30, "custom": "kept"})),
        ..Default::default()
    };
    let explicit = GamesService::create_game(&state, &body)
        .await
        .expect("an explicit engine and config register");
    assert_eq!(
        explicit.engine_type, "godot",
        "the whitelist is matched case-folded"
    );
    assert_eq!(explicit.config["tick_rate"], json!(30));
    assert_eq!(explicit.config["custom"], json!("kept"));
    assert_eq!(
        explicit.config.get("max_clients"),
        None,
        "a named config replaces the defaults instead of being merged into them"
    );

    db::forget(&pool, game.id).await;
    db::forget(&pool, explicit.id).await;
}
#[tokio::test]
pub(crate) async fn every_engine_derives_the_backend_mode_the_plan_assigns_it() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    for (engine, mode) in [
        ("unity", "relay"),
        ("unreal", "relay"),
        ("godot", "relay"),
        ("web", "wasm"),
        ("custom", "wasm"),
    ] {
        let game = db::create_engine(&state, &format!("mode-{engine}"), Some(engine)).await;
        assert_eq!(game.engine_type, engine);
        assert_eq!(
            game.backend_mode, mode,
            "`{engine}` must register on the `{mode}` backend"
        );
        let detail = GamesService::get_game(&state, game.id)
            .await
            .expect("the row is readable");
        assert_eq!(detail.backend_mode, mode, "and the row says so");
        db::forget(&pool, game.id).await;
    }
}
#[tokio::test]
pub(crate) async fn switching_engine_type_flips_the_derived_backend_mode_both_ways() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create_engine(&state, "mode-flip", Some("unity")).await;
    assert_eq!(game.backend_mode, "relay");

    let to_wasm: UpdateGameRequest =
        serde_json::from_value(json!({"engine_type": "custom"})).expect("an engine patch");
    let patched = GamesService::update_game(&state, game.id, &to_wasm, None)
        .await
        .expect("a Unity game can become an authoritative one");
    assert_eq!(patched.engine_type, "custom");
    assert_eq!(
        patched.backend_mode, "wasm",
        "and its mode follows the engine, because no body named a mode"
    );

    let back: UpdateGameRequest =
        serde_json::from_value(json!({"engine_type": "godot"})).expect("a second engine patch");
    let patched = GamesService::update_game(&state, game.id, &back, None)
        .await
        .expect("and the way back is the same patch");
    assert_eq!(patched.engine_type, "godot");
    assert_eq!(patched.backend_mode, "relay");

    let renamed: UpdateGameRequest =
        serde_json::from_value(json!({"name": "Renamed Relay"})).expect("a name patch");
    let patched = GamesService::update_game(&state, game.id, &renamed, None)
        .await
        .expect("a rename is applied");
    assert_eq!(patched.backend_mode, "relay", "a rename keeps the mode");
    assert_eq!(patched.engine_type, "godot");

    db::forget(&pool, game.id).await;
}
