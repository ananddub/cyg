use super::*;

#[tokio::test]
pub(crate) async fn a_game_key_is_unique_and_a_name_is_not() {
    let Some((_state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let key = format!(
        "{GAME_KEY_PREFIX}unique_probe_{}",
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let insert = "CREATE type::record('games', <uuid>$id) CONTENT $row RETURN NOTHING";
    let write = |game_key: String, label: String| {
        let row = json!({
            "game_key": game_key,
            "name": label,
            "api_key": format!("{API_KEY_PREFIX}{label}"),
            "client_key": format!("{CLIENT_KEY_PREFIX}{label}"),
            "engine_type": "unity",
            "backend_mode": "relay",
            "status": "active",
            "config": {},
            "created_at": time::encode(Utc::now()),
            "updated_at": time::encode(Utc::now()),
        });
        let id = Uuid::new_v4().to_string();
        let pool = pool.clone();
        async move {
            pool.exec_params(
                insert,
                &crate::repositories::db::params! { id = id, row = row },
            )
            .await
        }
    };

    write(key.clone(), format!("{TEST_PREFIX} duplicate-a"))
        .await
        .expect("the first row with a key is written");

    let err = write(key.clone(), format!("{TEST_PREFIX} duplicate-b"))
        .await
        .expect_err("a duplicated game_key is refused by the unique index");
    assert!(
        crate::error::V2Error::is_any_unique_violation(&err),
        "and it is refused as a unique violation, not some other database error: {err}"
    );

    for _ in 0..2 {
        write(
            format!("{GAME_KEY_PREFIX}name_probe_{}", Uuid::new_v4().simple()),
            format!("{TEST_PREFIX} shared name"),
        )
        .await
        .unwrap_or_else(|err| panic!("a repeated name is a fine tenant: {err}"));
    }

    pool.exec_params(
        "DELETE games WHERE game_key = $key OR name = $name RETURN NOTHING",
        &crate::repositories::db::params! {
            key = key.clone(),
            name = format!("{TEST_PREFIX} shared name"),
        },
    )
    .await
    .expect("the fixtures are removed again");
}
#[tokio::test]
pub(crate) async fn the_listing_hides_the_secret_and_the_detail_reveals_it_with_counts() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "listing").await;
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the detail view reads the game back");

    let list = GamesService::list_games(&state)
        .await
        .expect("the listing reads");
    assert!(
        list.total >= 1,
        "the listing reports the size it is showing: {}",
        list.total
    );
    let row = list
        .games
        .iter()
        .find(|row| row.id == game.id)
        .expect("a freshly registered game is in the listing");
    assert_eq!(row.game_key, game.game_key);

    assert_eq!(row.client_key, game.client_key);
    assert_eq!(row.backend_mode, game.backend_mode, "and so does its mode");

    let wire: Value = serde_json::to_value(&list).expect("the listing serializes");
    assert!(
        wire["games"].is_array() && wire["total"].is_number(),
        "{wire}"
    );
    let text = wire.to_string();
    assert!(
        !text.contains("api_key") && !text.contains(API_KEY_PREFIX),
        "a listing must never carry a tenant secret: {text}"
    );
    assert!(
        text.contains(CLIENT_KEY_PREFIX),
        "…while the publishable client key it does carry is the one a build reads: {text}"
    );

    let detail_wire = serde_json::to_value(&detail).expect("the detail serializes");
    assert_eq!(detail_wire["api_key"], json!(detail.api_key));
    assert_eq!(detail_wire["client_key"], json!(detail.client_key));
    assert_eq!(detail_wire["backend_mode"], json!(detail.backend_mode));
    assert_eq!(
        detail_wire["active_module_version"],
        json!(null),
        "a game with no uploads has no version to boot at"
    );
    assert_eq!(detail_wire["module_count"], json!(0));
    assert!(
        detail_wire["config"].is_object(),
        "the detail carries the config the create wrote"
    );

    let missing = GamesService::get_game(&state, Uuid::new_v4()).await;
    let err = missing.expect_err("an unknown game id is not readable");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}
