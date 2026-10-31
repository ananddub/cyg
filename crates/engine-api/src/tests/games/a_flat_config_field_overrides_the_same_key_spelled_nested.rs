use super::*;

#[tokio::test]
pub(crate) async fn a_flat_config_field_overrides_the_same_key_spelled_nested() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let both = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("precedence"),
            "engine_type": "web",
            "tick_rate": 60,
            "auto_close_seconds": 12,
            "config": {"tick_rate": 20, "max_clients": 4, "auto_close_seconds": 99},
        }),
    )
    .await;
    assert_eq!(
        both.config["tick_rate"],
        json!(60),
        "the flat field beats the nested duplicate: {}",
        both.config
    );
    assert_eq!(
        both.config["auto_close_seconds"],
        json!(12),
        "for every key the flat body names: {}",
        both.config
    );
    assert_eq!(
        both.config["max_clients"],
        json!(4),
        "and a key only the nested document names is kept as written: {}",
        both.config
    );

    let flat = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("flat-only"),
            "engine_type": "custom",
            "persistent_rooms": true,
        }),
    )
    .await;
    assert_eq!(flat.config["persistent_rooms"], json!(true));
    assert_eq!(flat.backend_mode, "wasm", "a custom game is authoritative");

    let nested = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("nested-only"),
            "config": {"tick_rate": 15, "studio_owned": "kept"},
        }),
    )
    .await;
    assert_eq!(nested.config["tick_rate"], json!(15));
    assert_eq!(nested.config["studio_owned"], json!("kept"));

    for game in [&both, &flat, &nested] {
        db::forget(&pool, game.id).await;
    }
}
#[tokio::test]
pub(crate) async fn the_persisted_config_round_trips_the_room_tuning_fields() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("config-round-trip"),
            "engine_type": "custom",
            "config": {"tick_rate": 30, "max_clients": 4, "persistent_rooms": true,
                       "auto_close_seconds": 300},
        }),
    )
    .await;
    assert_eq!(game.config["tick_rate"], json!(30));
    assert_eq!(game.config["max_clients"], json!(4));
    assert_eq!(game.config["persistent_rooms"], json!(true));
    assert_eq!(game.config["auto_close_seconds"], json!(300));

    let stored: GameConfig =
        serde_json::from_value(game.config.clone()).expect("the stored config is a GameConfig");
    assert_eq!(
        stored,
        GameConfig {
            tick_rate: 30,
            max_clients: 4,
            persistent_rooms: true,
            auto_close_seconds: 300,
        }
    );

    let patched: UpdateGameRequest = serde_json::from_value(json!({"config": {
        "tick_rate": 120, "max_clients": 64, "persistent_rooms": false,
        "auto_close_seconds": 5,
    }}))
    .expect("a config patch");
    let patched = GamesService::update_game(&state, game.id, &patched)
        .await
        .expect("the second config is stored");
    assert_eq!(patched.config["tick_rate"], json!(120));
    assert_eq!(patched.config["persistent_rooms"], json!(false));
    assert_eq!(patched.config["auto_close_seconds"], json!(5));

    let renamed: UpdateGameRequest =
        serde_json::from_value(json!({"name": "Still The Same Tuning"})).expect("a patch");
    let kept = GamesService::update_game(&state, game.id, &renamed)
        .await
        .expect("a rename is applied");
    assert_eq!(
        kept.config, patched.config,
        "an absent config keeps what was stored"
    );

    db::forget(&pool, game.id).await;
}
