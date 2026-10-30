use super::*;

pub async fn season_gating(env: &Harness, console: &str) {
    eprintln!("gap: a quest only runs inside its season");
    let player = device_player(&env.client, &env.base, "season").await;
    let season = format!("winter-{}", letters_suffix());
    let quest = format!("frost-{}", letters_suffix());

    refusal(
        env.client
            .post(format!("{}/v2/console/quests", env.base))
            .bearer_auth(console)
            .json(&json!({
                "quest_key": quest,
                "title": "Frozen before it opened",
                "goal_kind": "kills",
                "goal_amount": 1,
                "season": "no-such-season",
                "reward": { "currency": { "gold": 5 } },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "point a quest at a season that does not exist",
    )
    .await;

    create_season(
        env,
        console,
        json!({
            "season_key": season,
            "title": "Winter",
            "starts_at": "2030-01-01T00:00:00Z",
            "ends_at": "2031-01-01T00:00:00Z",
            "reward_track": { "tiers": [10, 20] },
        }),
    )
    .await;
    create_quest(
        env,
        console,
        json!({
            "quest_key": quest,
            "title": "Frozen before it opened",
            "goal_kind": "kills",
            "goal_amount": 1,
            "season": season,
            "reward": { "currency": { "gold": 5 } },
        }),
    )
    .await;

    let outside = json_body(
        claim_quest(env, &player.token, &quest).await,
        StatusCode::BAD_REQUEST,
        "reading a closed quest is refused",
    )
    .await;
    assert!(
        outside["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("window"),
        "the refusal says why: {outside}"
    );
    let listed = quests_of(env, &player.token).await;
    assert!(
        !listed
            .iter()
            .any(|row| row["quest_key"] == json!(quest.as_str())),
        "a closed season hides its quest: {listed:?}"
    );

    // The counter is credited even while the quest is closed, so a player who
    // played during the gap is not cheated when the season opens.
    ingest(
        env,
        json!([{ "user_id": player.uid, "name": "kills", "value": 4 }]),
    )
    .await;

    write_season(
        env,
        console,
        &season,
        json!({
            "title": "Winter",
            "starts_at": "2020-01-01T00:00:00Z",
            "ends_at": "2031-01-01T00:00:00Z",
            "reward_track": { "tiers": [10, 20] },
        }),
    )
    .await;
    let open = quest_row(env, &player.token, &quest).await;
    assert_eq!(open["progress"], json!(4), "the season reads the counter");
    assert_eq!(open["state"], json!("completed"));

    ingest(
        env,
        json!([{ "user_id": player.uid, "name": "kills", "value": 1 }]),
    )
    .await;
    let after = quest_row(env, &player.token, &quest).await;
    assert_eq!(after["progress"], json!(5), "a later event still counts");

    let claimed = json_body(
        claim_quest(env, &player.token, &quest).await,
        StatusCode::OK,
        "claim the quest once its season is open",
    )
    .await;
    assert_eq!(claimed["reward"]["currency"]["gold"], json!(5.0));

    // Closing the season again stops the quest from being listed.
    write_season(
        env,
        console,
        &season,
        json!({
            "title": "Winter",
            "starts_at": "2030-01-01T00:00:00Z",
            "ends_at": "2031-01-01T00:00:00Z",
            "reward_track": { "tiers": [10, 20] },
        }),
    )
    .await;
    let listed = quests_of(env, &player.token).await;
    assert!(
        !listed
            .iter()
            .any(|row| row["quest_key"] == json!(quest.as_str())),
        "a season that ended hides its quest: {listed:?}"
    );
}
