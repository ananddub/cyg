use super::*;

#[tokio::test]
pub(crate) async fn activate_module_moves_the_flag_and_rollback_is_the_same_flip() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-activate").await;
    let (bytes, _) = complete_wasm();

    for version in [1, 2, 3] {
        service
            .insert_module(
                &state,
                game.id,
                params(&format!("room-v{version}.wasm"), &bytes, None),
            )
            .await
            .expect("three versions are stored");
    }
    let active: Vec<i32> = service
        .list_modules(&state, game.id)
        .await
        .expect("the listing reads")
        .iter()
        .filter(|module| module.is_active)
        .map(|module| module.version)
        .collect();
    assert_eq!(active, vec![3], "the third upload is the active one");

    let rolled_back = service
        .activate_module(&state, game.id, 1, engine_bus::ReloadKind::Rollback)
        .await
        .expect("an older version can be activated");
    assert_eq!(rolled_back.version, 1);
    assert!(rolled_back.is_active);
    let list = service
        .list_modules(&state, game.id)
        .await
        .expect("listing");
    assert_eq!(
        list.iter()
            .filter(|module| module.is_active)
            .map(|module| module.version)
            .collect::<Vec<_>>(),
        vec![1],
        "activating one version deactivates every other, including the newest"
    );

    let forward = service
        .activate_module(&state, game.id, 3, engine_bus::ReloadKind::Upload)
        .await
        .expect("activation is not one-directional");
    assert_eq!(forward.version, 3);
    let err = service
        .activate_module(&state, game.id, 99, engine_bus::ReloadKind::Rollback)
        .await
        .expect_err("a version nobody uploaded cannot be activated");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");
    assert_eq!(
        service
            .list_modules(&state, game.id)
            .await
            .expect("the listing still reads")
            .iter()
            .filter(|module| module.is_active)
            .count(),
        1,
        "a failed activation leaves the active version active"
    );

    db::forget(&pool, game.id).await;
}
#[tokio::test]
pub(crate) async fn get_module_bytes_round_trips_what_was_stored() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-bytes").await;
    let (bytes, _) = complete_wasm();

    service
        .insert_module(&state, game.id, params("room-v1.wasm", &bytes, None))
        .await
        .expect("version 1 is stored");

    let (filename, stored) = service
        .get_module_bytes(&state, game.id, 1)
        .await
        .expect("the hot-reload fetch reads the version it was told to load");
    assert_eq!(filename, "room-v1.wasm");
    assert_eq!(
        stored, bytes,
        "the database is a faithful store, not a transform"
    );

    let err = service
        .get_module_bytes(&state, game.id, 7)
        .await
        .expect_err("a version that was never uploaded has no bytes");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}
