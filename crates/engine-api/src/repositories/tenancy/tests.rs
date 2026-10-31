use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::repositories::games::InsertGameParams;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_game_is_readable_by_id_key_and_name() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let game = q
        .insert_game(&InsertGameParams {
            game_key: "tenant-game".into(),
            name: "Tenant Game".into(),
            api_key: "api-key".into(),
            client_key: "client-key".into(),
            engine_type: "custom".into(),
            backend_mode: "relay".into(),
            config: json!({}),
        })
        .await
        .expect("the game registers");

    assert_eq!(
        q.tenancy_game_by_id(game.id)
            .await
            .expect("by id reads")
            .len(),
        1
    );
    assert_eq!(
        q.tenancy_game_by_key("tenant-game")
            .await
            .expect("by key reads")
            .len(),
        1
    );
    assert_eq!(
        q.tenancy_game_by_name_or_key("Tenant Game")
            .await
            .expect("by name reads")
            .expect("the game exists")
            .id,
        game.id
    );
}

#[tokio::test]
async fn stamping_a_user_scopes_it_to_the_game() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let game = Uuid::new_v4();
    let user = Uuid::new_v4();
    q.create_user_bare(user, "tenant-user")
        .await
        .expect("the user registers");

    assert!(
        !q.tenancy_account_in_game(user, game)
            .await
            .expect("the scoped lookup runs")
    );
    q.stamp_user_game(game, user)
        .await
        .expect("the game stamps");
    assert!(
        q.tenancy_account_in_game(user, game)
            .await
            .expect("the scoped lookup runs")
    );
    let accounts = q
        .tenancy_accounts(Some(game), 10, 0)
        .await
        .expect("the scoped list runs");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, user);
}
