use super::*;

pub async fn event_ingest(env: &Harness, console: &str) {
    eprintln!("gap: the event door is server-only and strict");
    let player = device_player(&env.client, &env.base, "ingest").await;
    let quest = format!("cast-{}", letters_suffix());
    create_quest(
        env,
        console,
        json!({
            "quest_key": quest,
            "title": "Land ten casts",
            "goal_kind": "casts",
            "goal_amount": 10,
            "reward": { "currency": { "shards": 1 } },
        }),
    )
    .await;

    // A player session cannot credit its own progress.
    refusal(
        env.client
            .post(format!("{}/v2/events", env.base))
            .bearer_auth(&player.token)
            .json(&json!({ "events": [{ "user_id": player.uid, "name": "casts" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "ingest events with a player token",
    )
    .await;

    refusal(
        env.client
            .post(format!("{}/v2/events", env.base))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "events": [] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "ingest an empty batch",
    )
    .await;

    refusal(
        env.client
            .post(format!("{}/v2/events", env.base))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "events": [{ "user_id": uid(), "name": "casts" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "credit an account that does not exist",
    )
    .await;

    for (name, value, what) in [
        ("Casts", None, "a counter name with a capital"),
        ("casts", Some(0), "a zero increment"),
        ("casts", Some(1_000_001), "an increment above the cap"),
    ] {
        let mut event = json!({ "user_id": player.uid, "name": name });
        if let Some(value) = value {
            event["value"] = json!(value);
        }
        refusal(
            env.client
                .post(format!("{}/v2/events", env.base))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "events": [event] }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }

    // One bad event in a batch refuses the batch before any of it is applied.
    refusal(
        env.client
            .post(format!("{}/v2/events", env.base))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "events": [
                { "user_id": player.uid, "name": "casts", "value": 3 },
                { "user_id": player.uid, "name": "Bad Name" },
            ] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a batch holding one unusable event",
    )
    .await;
    let untouched = quest_row(env, &player.token, &quest).await;
    assert_eq!(
        untouched["progress"],
        json!(0),
        "the rejected batch credited nothing: {untouched}"
    );

    let accepted = ingest(
        env,
        json!([
            { "user_id": player.uid, "name": "casts", "value": 3 },
            { "user_id": player.uid, "name": "casts", "value": 4 },
            { "user_id": player.uid, "name": "crits" },
        ]),
    )
    .await;
    assert_eq!(accepted["accepted"], json!(3));
    assert_eq!(accepted["completed"], json!([]));
    let row = quest_row(env, &player.token, &quest).await;
    assert_eq!(row["progress"], json!(7));
    assert_eq!(row["state"], json!("in_progress"));
    assert_eq!(
        rows_matching(env, "player_stat", "\"crits\"").await.len(),
        1,
        "a counter with no quest still accumulates"
    );
}
