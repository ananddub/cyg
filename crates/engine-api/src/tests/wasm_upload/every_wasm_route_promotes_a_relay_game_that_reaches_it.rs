use super::*;

#[tokio::test]
pub(crate) async fn every_wasm_route_promotes_a_relay_game_that_reaches_it() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let (bytes, _) = complete_wasm();

    let game = db::create_engine(&state, "wasm-promote-upload", Some("unity")).await;
    assert_eq!(game.backend_mode, "relay", "the fixture is a relay game");
    service
        .upload_module(&state, game.id, "room.wasm".to_owned(), bytes.clone(), None)
        .await
        .expect("reaching the surface promotes the game and stores the module");
    let detail = crate::services::games::GamesService::get_game(&state, game.id)
        .await
        .expect("the game reads back");
    assert_eq!(
        detail.backend_mode, "wasm",
        "the promotion is stored, not just felt"
    );
    assert_eq!(detail.module_count, 1, "and the module landed");

    let listed = db::create_engine(&state, "wasm-promote-list", Some("unity")).await;
    let modules = service
        .list_modules(&state, listed.id)
        .await
        .expect("a relay game's listing is answered, not refused");
    assert!(modules.is_empty(), "answered with the modules it has: {modules:?}");
    let detail = crate::services::games::GamesService::get_game(&state, listed.id)
        .await
        .expect("the listed game reads back");
    assert_eq!(detail.backend_mode, "wasm", "and the contact promoted it");

    let activated = db::create_engine(&state, "wasm-promote-activate", Some("unity")).await;
    let err = service
        .activate_module(&state, activated.id, 1, engine_bus::ReloadKind::Upload)
        .await
        .expect_err("there is no version 1 to activate");
    assert_eq!(
        err.kind,
        crate::error::NOT_FOUND,
        "the refusal is about the missing module, not the mode: {err:?}"
    );
    let detail = crate::services::games::GamesService::get_game(&state, activated.id)
        .await
        .expect("the activated game reads back");
    assert_eq!(detail.backend_mode, "wasm");

    let err = service
        .upload_module(&state, Uuid::new_v4(), "room.wasm".to_owned(), bytes, None)
        .await
        .expect_err("a game that is not there cannot be promoted");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
    db::forget(&pool, listed.id).await;
    db::forget(&pool, activated.id).await;
}
#[tokio::test]
pub(crate) async fn the_wasm_gate_promotes_the_stored_mode_on_every_contact() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_engine(&state, "wasm-switch", Some("godot")).await;
    let (bytes, _) = complete_wasm();

    service
        .upload_module(&state, game.id, "room.wasm".to_owned(), bytes.clone(), None)
        .await
        .expect("a godot game is promoted by its first upload");
    let detail = crate::services::games::GamesService::get_game(&state, game.id)
        .await
        .expect("the game reads back");
    assert_eq!(detail.backend_mode, "wasm");

    let back: crate::dto::console::games::UpdateGameRequest =
        serde_json::from_value(serde_json::json!({"engine_type": "unity"})).expect("a patch back");
    let moved = crate::services::games::GamesService::update_game(&state, game.id, &back)
        .await
        .expect("the console moves the game back to the relay");
    assert_eq!(moved.backend_mode, "relay", "the patch is honest about the plane");

    service
        .list_modules(&state, game.id)
        .await
        .expect("and the next contact is answered rather than refused");
    let detail = crate::services::games::GamesService::get_game(&state, game.id)
        .await
        .expect("the game reads back again");
    assert_eq!(
        detail.backend_mode, "wasm",
        "asking the surface re-promotes the stored mode"
    );

    db::forget(&pool, game.id).await;
}
