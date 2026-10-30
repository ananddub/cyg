use super::*;

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
    let game = GamesService::create_game(&state, &body)
        .await
        .expect("a game with an explicit engine and config registers");

    let renamed: UpdateGameRequest = serde_json::from_value(json!({"name": "Renamed Arena"}))
        .expect("a name-only patch deserializes");
    let patched = GamesService::update_game(&state, game.id, &renamed, None)
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
    let patched = GamesService::update_game(&state, game.id, &patched, None)
        .await
        .expect("the second patch is applied");
    assert_eq!(patched.status, "suspended");
    assert_eq!(patched.config["tick_rate"], json!(64));

    let err = GamesService::update_game(&state, Uuid::new_v4(), &renamed, None)
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
    let archived = GamesService::archive_game(&state, game.id)
        .await
        .expect("retiring a game is a service call, not a delete statement");
    assert!(archived, "and it reports that it archived something");

    let row = GamesService::find_game(&state, game.id)
        .await
        .expect("the row is still there, which is the whole point of a soft delete");
    assert_eq!(row.status, "archived");
    assert_eq!(
        row.game_key, game.game_key,
        "a retired tenant keeps its identity"
    );

    let list = GamesService::list_games(&state)
        .await
        .expect("the listing reads");
    assert!(
        list.games.iter().any(|row| row.id == game.id),
        "an archived game stays in the listing"
    );
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the archived game is still readable in full");
    assert!(detail.api_key.starts_with(API_KEY_PREFIX));

    assert!(
        GamesService::archive_game(&state, game.id)
            .await
            .expect("idempotent")
    );
    let err = GamesService::archive_game(&state, Uuid::new_v4())
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
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the secret is read from the detail view");

    assert!(
        GamesService::verify_api_key(&state, &detail.game_key, &detail.api_key)
            .await
            .expect("the pair the console just printed authenticates")
    );
    let wrong_secret = GamesService::verify_api_key(&state, &detail.game_key, "cyg_sec_wrong")
        .await
        .expect("a wrong secret is an authentication answer, not an error");
    assert!(!wrong_secret);
    let unknown_key =
        GamesService::verify_api_key(&state, "gm_no_such_game_000000", &detail.api_key)
            .await
            .expect("an unknown tenant key is the same kind of answer");
    assert!(
        !unknown_key,
        "an unknown key and a wrong secret must not be distinguishable"
    );

    let row = GamesService::find_game_by_key(&state, &detail.game_key)
        .await
        .expect("a lookup by tenant key reads")
        .expect("and finds the game that key was minted for");
    assert_eq!(row.id, game.id);
    assert!(
        GamesService::find_game_by_key(&state, "gm_never_minted_000000")
            .await
            .expect("an unknown key is a `None`, not an error")
            .is_none()
    );

    db::forget(&pool, game.id).await;
}
