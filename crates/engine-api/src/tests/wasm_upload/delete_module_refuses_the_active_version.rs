use super::*;

#[tokio::test]
pub(crate) async fn delete_module_refuses_the_active_version() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-delete").await;
    let (bytes, _) = complete_wasm();

    for version in [1, 2] {
        service
            .insert_module(
                &state,
                game.id,
                params(&format!("room-v{version}.wasm"), &bytes, None),
            )
            .await
            .expect("two versions, the second active");
    }

    service
        .delete_module(&state, game.id, 1)
        .await
        .expect("an inactive version is deletable");
    let list = service
        .list_modules(&state, game.id)
        .await
        .expect("listing");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].version, 2);

    let err = service
        .delete_module(&state, game.id, 2)
        .await
        .expect_err("deleting the active module would strand every live room");
    assert_eq!(err.kind, crate::error::FAILED_PRECONDITION, "{err:?}");
    assert!(
        err.message.contains("active"),
        "and the refusal says what to do instead: {}",
        err.message
    );
    assert_eq!(
        service
            .list_modules(&state, game.id)
            .await
            .expect("listing")
            .len(),
        1,
        "the refusal did not delete anything"
    );

    let fresh = service
        .insert_module(&state, game.id, params("room-v3.wasm", &bytes, None))
        .await
        .expect("a third upload becomes active");
    assert_eq!(fresh.version, 3);
    assert!(fresh.is_active);
    service
        .delete_module(&state, game.id, 2)
        .await
        .expect("version 2 is inactive now, and so is removable");

    let err = service
        .delete_module(&state, game.id, 42)
        .await
        .expect_err("deleting a version nobody uploaded is not a success");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}
#[tokio::test]
pub(crate) async fn upload_module_gates_on_tenant_then_abi_then_stores() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-upload").await;
    let (bytes, _) = complete_wasm();

    let err = service
        .upload_module(
            &state,
            uuid::Uuid::new_v4(),
            "room.wasm".to_owned(),
            bytes.clone(),
            None,
        )
        .await
        .expect_err("no tenant, no module");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    let err = service
        .upload_module(&state, game.id, String::new(), bytes.clone(), None)
        .await
        .expect_err("an upload has to name its file");
    assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");

    let err = service
        .upload_module(&state, game.id, "hello.wasm".to_owned(), empty_wasm(), None)
        .await
        .expect_err("an empty module is not a game host");
    assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");
    assert_eq!(
        crate::services::games::GamesService::module_count(&state, game.id)
            .await
            .expect("the count reads"),
        0,
        "a refused upload leaves no row behind"
    );

    let response = service
        .upload_module(
            &state,
            game.id,
            "room-logic.wasm".to_owned(),
            bytes.clone(),
            Some("export function on_tick(dt: f64): i64 { return 0; }".to_owned()),
        )
        .await
        .expect("a module with the whole ABI uploads");
    assert_eq!(response.module.version, 1);
    assert!(response.module.is_active);
    assert_eq!(response.module.filename, "room-logic.wasm");
    assert_eq!(response.module.sha256, sha256_of(&bytes));
    assert!(response.abi.valid, "the report says what was checked");
    assert!(response.abi.exports_missing.is_empty());

    let wire = serde_json::to_value(&response).expect("the upload response serializes");
    assert!(wire["module"]["version"].is_number(), "{wire}");
    assert_eq!(wire["abi"]["valid"], serde_json::json!(true));
    assert!(wire["abi"]["exports_found"].is_array(), "{wire}");

    let (filename, stored) = service
        .get_module_bytes(&state, game.id, 1)
        .await
        .expect("the stored version is fetchable");
    assert_eq!(filename, "room-logic.wasm");
    assert_eq!(stored, bytes);

    db::forget(&pool, game.id).await;
}
