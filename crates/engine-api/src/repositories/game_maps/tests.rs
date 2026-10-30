use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_game_map_versions_activates_and_deletes() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let game = Uuid::new_v4();
    let id = Uuid::new_v4();
    let key = format!("maps/{game}/1.map3d");

    q.insert_game_map(
        game,
        id,
        "map3d",
        &json!({
            "game_id": game.to_string(),
            "version": 1,
            "name": "Arena",
            "format": "map3d",
            "sha256": "abc",
            "blob_key": key,
            "size_bytes": 12,
            "is_active": true,
            "created_at": crate::db::time::encode(Utc::now()),
        }),
    )
    .await
    .expect("the map inserts");

    assert_eq!(
        q.latest_game_map_version(game)
            .await
            .expect("the version reads"),
        Some(1)
    );
    let row = q
        .game_map_at_version(game, 1)
        .await
        .expect("the row reads")
        .expect("the row exists");
    assert_eq!(row.blob_key.as_deref(), Some(key.as_str()));

    q.activate_game_map(game, 1, "map3d")
        .await
        .expect("the map activates");
    assert_eq!(
        q.game_map_active_flag(game, 1)
            .await
            .expect("the flag reads"),
        vec![true]
    );

    let deleted = q.delete_game_map(game, 1).await.expect("the map deletes");
    assert_eq!(deleted.len(), 1);
    assert!(
        q.list_game_maps(game)
            .await
            .expect("the listing runs")
            .is_empty()
    );
}
