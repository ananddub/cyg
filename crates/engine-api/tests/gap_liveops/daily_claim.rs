use super::*;

pub async fn daily_claim(env: &Harness, console: &str) {
    eprintln!("gap: one daily reward per UTC day");
    let player = device_player(&env.client, &env.base, "daily").await;

    let first = json_body(
        claim_daily(env, &player.token).await,
        StatusCode::OK,
        "claim the daily reward",
    )
    .await;
    assert_eq!(first["success"], json!(true));
    assert_eq!(
        first["reward"]["currency"]["gold"],
        json!(100.0),
        "the built-in daily reward: {first}"
    );
    let day = first["day"].as_i64().expect("a day number");
    assert!(day > 0, "the day is a real day number: {day}");
    assert_eq!(
        wallet_of(env, console, &player.uid).await["gold"],
        json!(100.0)
    );

    refusal(
        claim_daily(env, &player.token).await,
        StatusCode::CONFLICT,
        "claim the daily reward again the same day",
    )
    .await;
    assert_eq!(
        rows_matching(env, "wallet_ledger", &player.uid).await.len(),
        1,
        "the refused claim wrote no second ledger row"
    );

    // An operator can set the daily reward, and the next day pays it.
    json_body(
        env.client
            .put(format!("{}/v2/console/setting", env.base))
            .bearer_auth(console)
            .json(&json!({
                "name": "daily_reward",
                "value": { "currency": { "gold": 7 }, "items": [] },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "set the daily reward",
    )
    .await;
    env.db()
        .exec("UPDATE player_stat SET value = 0 WHERE stat = 'daily_claim'")
        .await
        .expect("roll the day back");

    let next = json_body(
        claim_daily(env, &player.token).await,
        StatusCode::OK,
        "claim the daily reward the next day",
    )
    .await;
    assert!(
        next["day"].as_i64().expect("a day number") >= day,
        "the marker was stamped again: {next}"
    );
    assert_eq!(next["reward"]["currency"]["gold"], json!(7.0));
    assert_eq!(
        wallet_of(env, console, &player.uid).await["gold"],
        json!(107.0),
        "the next day paid the configured reward"
    );

    // An empty configured reward is refused rather than paying nothing.
    json_body(
        env.client
            .put(format!("{}/v2/console/setting", env.base))
            .bearer_auth(console)
            .json(&json!({ "name": "daily_reward", "value": { "currency": {} } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "empty the daily reward",
    )
    .await;
    env.db()
        .exec("UPDATE player_stat SET value = 0 WHERE stat = 'daily_claim'")
        .await
        .expect("roll the day back");
    let empty = refusal(
        claim_daily(env, &player.token).await,
        StatusCode::BAD_REQUEST,
        "claim an empty daily reward",
    )
    .await;
    assert!(
        empty.contains("No daily reward is configured"),
        "the refusal says why: {empty}"
    );
}
