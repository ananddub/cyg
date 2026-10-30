use super::*;

pub async fn quest_lifecycle(env: &Harness, console: &str) {
    eprintln!("gap: event -> progress -> claim -> reward");
    let player = device_player(&env.client, &env.base, "quest").await;
    let item = format!("quest-medal-{}", letters_suffix());
    define_item(env, console, &item, 5).await;

    let quest = format!("win-{}", letters_suffix());
    let definition = json!({
        "quest_key": quest,
        "title": "Win three matches",
        "goal_kind": "wins",
        "goal_amount": 3,
        "reward": {
            "currency": { "gold": 50 },
            "items": [{ "item_id": item.clone(), "quantity": 2 }],
        },
    });
    let created = create_quest(env, console, definition.clone()).await;
    assert_eq!(created["quest_key"], json!(quest));
    assert_eq!(created["title"], json!("Win three matches"));

    refusal(
        env.client
            .post(format!("{}/v2/console/quests", env.base))
            .bearer_auth(console)
            .json(&definition)
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "define the same quest key twice",
    )
    .await;

    ingest(
        env,
        json!([{ "user_id": player.uid, "name": "wins", "value": 1 }]),
    )
    .await;
    let started = quest_row(env, &player.token, &quest).await;
    assert_eq!(started["progress"], json!(1));
    assert_eq!(started["state"], json!("in_progress"));
    refusal(
        claim_quest(env, &player.token, &quest).await,
        StatusCode::BAD_REQUEST,
        "claim a quest that is not finished",
    )
    .await;

    let crossed = ingest(
        env,
        json!([{ "user_id": player.uid, "name": "wins", "value": 2 }]),
    )
    .await;
    assert_eq!(
        crossed["completed"],
        json!([{ "user_id": player.uid, "quest_key": quest }]),
        "the event that crosses the goal reports it: {crossed}"
    );
    assert_eq!(
        quest_row(env, &player.token, &quest).await["state"],
        "completed"
    );

    let claimed = json_body(
        claim_quest(env, &player.token, &quest).await,
        StatusCode::OK,
        "claim the finished quest",
    )
    .await;
    assert_eq!(claimed["quest"]["state"], json!("claimed"));
    assert_eq!(claimed["reward"]["currency"]["gold"], json!(50.0));
    assert_eq!(
        claimed["reward"]["items"],
        json!([{ "item_id": item, "quantity": 2 }])
    );
    assert_eq!(
        wallet_of(env, console, &player.uid).await["gold"],
        json!(50.0),
        "the currency reward landed"
    );
    let items = inventory_of(env, &player.token).await;
    assert_eq!(items.len(), 1, "the item reward landed: {items:?}");
    assert_eq!(items[0]["item_id"], json!(item));
    assert_eq!(items[0]["quantity"], json!(2));

    refusal(
        claim_quest(env, &player.token, &quest).await,
        StatusCode::BAD_REQUEST,
        "claim the same quest twice",
    )
    .await;

    // More of the same counter cannot buy a second reward.
    ingest(
        env,
        json!([{ "user_id": player.uid, "name": "wins", "value": 5 }]),
    )
    .await;
    let after = quest_row(env, &player.token, &quest).await;
    assert_eq!(after["state"], json!("claimed"));
    assert_eq!(after["progress"], json!(3));
    assert_eq!(
        wallet_of(env, console, &player.uid).await["gold"],
        json!(50.0)
    );
    assert_eq!(inventory_of(env, &player.token).await.len(), 1);
    assert_eq!(
        rows_matching(env, "wallet_ledger", &player.uid).await.len(),
        1,
        "one claim wrote one ledger row"
    );
}
