use super::*;
use crate::error::FAILED_PRECONDITION;

#[tokio::test]
pub(crate) async fn update_game_changes_only_the_fields_a_patch_names() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let body = CreateGameRequest {
        name: db::fixture_name("patch"),
        engine_type: Some("web".to_owned()),
        config: Some(json!({"tick_rate": 20})),
        ..Default::default()
    };
    let game = GamesService::new(state.plane())
        .create_game(&body)
        .await
        .expect("a game with an explicit engine and config registers");

    let renamed: UpdateGameRequest = serde_json::from_value(json!({"name": "Renamed Arena"}))
        .expect("a name-only patch deserializes");
    let patched = GamesService::new(state.plane())
        .update_game(game.id, &renamed, None)
        .await
        .expect("a patch that names one field is applied");

    assert_eq!(patched.name, "Renamed Arena");
    assert_eq!(
        patched.game_key, game.game_key,
        "the tenant key is not editable"
    );
    assert_eq!(
        patched.engine_type, "web",
        "an absent engine keeps what was stored"
    );
    assert_eq!(
        patched.config["tick_rate"],
        json!(20),
        "an absent config keeps what was stored, not the defaults"
    );
    assert!(patched.updated_at >= game.updated_at);

    let patched: UpdateGameRequest =
        serde_json::from_value(json!({"status": "suspended", "config": {"tick_rate": 64}}))
            .expect("a status-plus-config patch deserializes");
    let patched = GamesService::new(state.plane())
        .update_game(game.id, &patched, None)
        .await
        .expect("the second patch is applied");
    assert_eq!(patched.status, "suspended");
    assert_eq!(patched.config["tick_rate"], json!(64));

    let err = GamesService::new(state.plane())
        .update_game(Uuid::new_v4(), &renamed, None)
        .await
        .expect_err("a patch to no game is not a write");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}
#[tokio::test]
pub(crate) async fn archive_game_is_a_soft_delete_that_keeps_the_row_and_its_key() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "archive").await;
    let archived = GamesService::new(state.plane())
        .archive_game(game.id)
        .await
        .expect("retiring a game is a service call, not a delete statement");
    assert!(archived, "and it reports that it archived something");

    let row = GamesService::new(state.plane())
        .find_game(game.id)
        .await
        .expect("the row is still there, which is the whole point of a soft delete");
    assert_eq!(row.status, "archived");
    assert_eq!(
        row.game_key, game.game_key,
        "a retired tenant keeps its identity"
    );

    let list = GamesService::new(state.plane())
        .list_games()
        .await
        .expect("the listing reads");
    assert!(
        list.games.iter().any(|row| row.id == game.id),
        "an archived game stays in the listing"
    );
    let detail = GamesService::new(state.plane())
        .get_game(game.id)
        .await
        .expect("the archived game is still readable in full");
    assert!(detail.api_key.starts_with(API_KEY_PREFIX));

    assert!(
        GamesService::new(state.plane())
            .archive_game(game.id)
            .await
            .expect("idempotent")
    );
    let err = GamesService::new(state.plane())
        .archive_game(Uuid::new_v4())
        .await
        .expect_err("retiring no game is not a success");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}
#[tokio::test]
pub(crate) async fn verify_api_key_authenticates_the_pair_and_refuses_probing() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "secret").await;
    let detail = GamesService::new(state.plane())
        .get_game(game.id)
        .await
        .expect("the secret is read from the detail view");

    assert!(
        GamesService::new(state.plane())
            .verify_api_key(&detail.game_key, &detail.api_key)
            .await
            .expect("the pair the console just printed authenticates")
    );
    let wrong_secret = GamesService::new(state.plane())
        .verify_api_key(&detail.game_key, "cyg_sec_wrong")
        .await
        .expect("a wrong secret is an authentication answer, not an error");
    assert!(!wrong_secret);
    let unknown_key = GamesService::new(state.plane())
        .verify_api_key("gm_no_such_game_000000", &detail.api_key)
        .await
        .expect("an unknown tenant key is the same kind of answer");
    assert!(
        !unknown_key,
        "an unknown key and a wrong secret must not be distinguishable"
    );

    let row = GamesService::new(state.plane())
        .find_game_by_key(&detail.game_key)
        .await
        .expect("a lookup by tenant key reads")
        .expect("and finds the game that key was minted for");
    assert_eq!(row.id, game.id);
    assert!(
        GamesService::new(state.plane())
            .find_game_by_key("gm_never_minted_000000")
            .await
            .expect("an unknown key is a `None`, not an error")
            .is_none()
    );

    db::forget(&pool, game.id).await;
}

#[tokio::test]
pub(crate) async fn validate_only_returns_the_planned_row_and_writes_nothing() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let game = db::create(&state, "validate").await;
    let planned: UpdateGameRequest = serde_json::from_value(json!({
        "name": "Only In The Reply",
        "validateOnly": true,
    }))
    .expect("a validation request deserializes");
    let preview = GamesService::new(state.plane())
        .update_game(game.id, &planned, None)
        .await
        .expect("a validation request is safe");
    assert_eq!(preview.name, "Only In The Reply");
    let stored = GamesService::new(state.plane())
        .find_game(game.id)
        .await
        .expect("the row is readable");
    assert_eq!(
        stored.name, game.name,
        "validateOnly must not touch the row"
    );
    db::forget(&pool, game.id).await;
}

#[tokio::test]
pub(crate) async fn undelete_restores_an_archived_game_only_once() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let game = db::create(&state, "undelete").await;
    let service = GamesService::new(state.plane());
    assert!(service.archive_game(game.id).await.expect("archive"));
    let restored = service.restore_game(game.id).await.expect("undelete");
    assert_eq!(restored.status, "active");
    let again = service
        .restore_game(game.id)
        .await
        .expect_err("an active game has nothing to restore");
    assert_eq!(again.kind, FAILED_PRECONDITION, "{again:?}");
    db::forget(&pool, game.id).await;
}

#[tokio::test]
pub(crate) async fn batch_get_and_batch_archive_keep_the_callers_order() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let first = db::create(&state, "batch-one").await;
    let second = db::create(&state, "batch-two").await;
    let service = GamesService::new(state.plane());
    let games = service
        .batch_get(&[second.id, first.id])
        .await
        .expect("both games exist");
    assert_eq!(games[0].id, second.id);
    assert_eq!(games[1].id, first.id);

    let missing = service
        .batch_get(&[first.id, Uuid::new_v4()])
        .await
        .expect_err("a batch with a missing id is refused");
    assert_eq!(missing.kind, NOT_FOUND, "{missing:?}");

    let archived = service
        .batch_archive(&[first.id, second.id, first.id])
        .await
        .expect("the batch archives once per unique id");
    assert_eq!(archived, 2);
    assert_eq!(
        service.find_game(first.id).await.expect("first row").status,
        "archived"
    );
    assert_eq!(
        service
            .find_game(second.id)
            .await
            .expect("second row")
            .status,
        "archived"
    );
    db::forget(&pool, first.id).await;
    db::forget(&pool, second.id).await;
}

#[tokio::test]
pub(crate) async fn criteria_delete_refuses_an_empty_filter_and_archives_matches() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let marker = format!("criteria-{}", Uuid::new_v4().simple());
    let first = db::create(&state, &format!("{marker}-one")).await;
    let second = db::create(&state, &format!("{marker}-two")).await;
    let service = GamesService::new(state.plane());

    let empty = service
        .archive_matching(None, None)
        .await
        .expect_err("an empty filter must not archive the fleet");
    assert_eq!(empty.kind, INVALID_ARGUMENT, "{empty:?}");

    let archived = service
        .archive_matching(None, Some(&marker))
        .await
        .expect("the criteria archive runs");
    assert!(archived >= 2, "both marked games are archived");
    assert_eq!(
        service.find_game(first.id).await.expect("first row").status,
        "archived"
    );
    assert_eq!(
        service
            .find_game(second.id)
            .await
            .expect("second row")
            .status,
        "archived"
    );
    db::forget(&pool, first.id).await;
    db::forget(&pool, second.id).await;
}
