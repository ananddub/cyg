use super::*;

#[tokio::test]
pub(crate) async fn the_module_aggregates_read_the_room_modules_rows() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "modules").await;
    let sha = hex::encode([7u8; 32]);

    db::insert_module(
        &state,
        &pool,
        game.id,
        1,
        "room-v1.wasm",
        &wasm_bytes(),
        None,
        &sha,
        true,
    )
    .await
    .expect("the first version of a module is stored");

    assert_eq!(
        GamesService::module_count(&state, game.id)
            .await
            .expect("the count reads"),
        1,
        "one upload is one module"
    );
    assert_eq!(
        GamesService::active_module_version(&state, game.id)
            .await
            .expect("the active version reads"),
        Some(1),
        "and the newest active one is what a room boots at"
    );

    let err = db::insert_module(
        &state,
        &pool,
        game.id,
        1,
        "room-v1-again.wasm",
        &wasm_bytes(),
        Some("source kept"),
        &sha,
        false,
    )
    .await
    .expect_err("(game_id, version) is unique");
    assert!(
        crate::error::V2Error::is_any_unique_violation(&err),
        "{err}"
    );

    let v2_key = format!("wasm/{}/2.wasm", game.id);
    state
        .blobs
        .as_ref()
        .expect("the test plane has a blob store")
        .put(&v2_key, &wasm_bytes())
        .await
        .expect("the second payload stores");
    let row = json!({
        "game_id": game.id.to_string(),
        "version": 2,
        "filename": "room-v2.wasm",
        "blob_key": v2_key,
        "size_bytes": wasm_bytes().len() as i64,
        "source_code": "source kept",
        "sha256": sha,
        "is_active": true,
        "created_at": time::encode(Utc::now()),
    });
    let id = Uuid::new_v4().to_string();
    pool.exec_params(
        "UPDATE room_modules SET is_active = false WHERE game_id = $game RETURN NOTHING;
         CREATE type::record('room_modules', <uuid>$id) CONTENT $row RETURN NOTHING",
        &crate::repositories::db::params! { game = game.id.to_string(), id = id, row = row },
    )
    .await
    .expect("both statements land together");

    assert_eq!(
        GamesService::module_count(&state, game.id)
            .await
            .expect("count"),
        2,
        "a newer version adds to the history rather than replacing it"
    );
    assert_eq!(
        GamesService::active_module_version(&state, game.id)
            .await
            .expect("version"),
        Some(2),
        "the newest active version wins"
    );

    pool.exec_params(
        "UPDATE room_modules SET is_active = false WHERE game_id = $game RETURN NOTHING",
        &crate::repositories::db::params! { game = game.id.to_string() },
    )
    .await
    .expect("deactivating every version is representable");
    assert_eq!(
        GamesService::active_module_version(&state, game.id)
            .await
            .expect("version"),
        None,
        "a game with modules but no active one boots nothing"
    );

    let read = |version: i32| {
        let pool = pool.clone();
        let game_id = game.id.to_string();
        async move {
            pool.query_params::<Value>(
                "SELECT * FROM room_modules
                 WHERE game_id = $game AND version = $version LIMIT 1",
                &crate::repositories::db::params! {
                    game = game_id, version = version,
                },
            )
            .await
            .expect("the version is readable")
            .into_iter()
            .next()
            .expect("and present")
        }
    };
    let store = state
        .blobs
        .as_ref()
        .expect("the test plane has a blob store");
    let v1 = read(1).await;
    assert_eq!(
        v1["blob_key"],
        json!(format!("wasm/{}/1.wasm", game.id)),
        "the key names the game and the version"
    );
    assert_eq!(v1["size_bytes"], json!(wasm_bytes().len() as i64));
    assert!(
        v1.get("binary_bytes").is_none(),
        "the payload table column is gone: {v1}"
    );
    let stored = store
        .get(v1["blob_key"].as_str().expect("a blob key"))
        .await
        .expect("the blob reads")
        .expect("and is present");
    assert_eq!(
        stored,
        wasm_bytes(),
        "the bytes the uploader sent are the bytes a room boots"
    );
    assert_eq!(v1["sha256"], json!(sha));
    assert!(
        v1.get("source_code").map_or(true, Value::is_null),
        "an upload that sent no source stores no field (or none), not an empty string"
    );
    let v2 = read(2).await;
    assert_eq!(
        v2["source_code"],
        json!("source kept"),
        "an uploader that sent source has it stored verbatim"
    );

    db::forget(&pool, game.id).await;
    let left = pool
        .query_params::<i64>(
            "RETURN array::len((SELECT id FROM room_modules WHERE game_id = $game))",
            &crate::repositories::db::params! { game = game.id.to_string() },
        )
        .await
        .expect("the module count reads");
    assert_eq!(
        left.into_iter().next().unwrap_or(0),
        2,
        "the history outlives a raw game delete; only archiving keeps it honest"
    );
}
