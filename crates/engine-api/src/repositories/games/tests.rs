use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_game_registers_and_reads_back_by_id_and_key() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let row = q
        .insert_game(&InsertGameParams {
            game_key: "gap-game".into(),
            name: "Gap Game".into(),
            api_key: "api-key".into(),
            client_key: "client-key".into(),
            engine_type: "custom".into(),
            backend_mode: "relay".into(),
            config: json!({}),
        })
        .await
        .expect("the game registers");

    assert_eq!(
        q.find_game_by_id(row.id)
            .await
            .expect("the game reads")
            .len(),
        1
    );
    assert_eq!(
        q.find_game_by_key("gap-game")
            .await
            .expect("the key reads")
            .len(),
        1
    );
    assert_eq!(q.count_games().await.expect("the count runs"), 1);
}

#[tokio::test]
async fn room_modules_version_activate_and_delete() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let game = Uuid::new_v4();
    let key = format!("wasm/{game}/1.wasm");
    q.insert_room_module(
        game,
        Uuid::new_v4(),
        &json!({
            "game_id": game.to_string(),
            "version": 1,
            "filename": "module.wasm",
            "blob_key": key,
            "size_bytes": 8,
            "sha256": "abc",
            "is_active": true,
            "created_at": crate::db::time::encode(Utc::now()),
        }),
    )
    .await
    .expect("the module inserts");

    assert_eq!(
        q.latest_room_module_version(game)
            .await
            .expect("the version reads"),
        Some(1)
    );
    let row = q
        .room_module_at_version(game, 1)
        .await
        .expect("the row reads")
        .expect("the row exists");
    assert_eq!(row.size_bytes, 8);
    assert_eq!(row.has_source, 0);

    q.activate_room_module(game, 1)
        .await
        .expect("the module activates");
    assert_eq!(
        q.room_module_active_flag(game, 1)
            .await
            .expect("the flag reads"),
        vec![true]
    );

    let deleted = q
        .delete_room_module(game, 1)
        .await
        .expect("the module deletes");
    assert_eq!(deleted[0].blob_key.as_deref(), Some(key.as_str()));
}
