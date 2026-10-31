use super::*;

pub async fn player_surface(env: &Harness) {
    eprintln!("gap: player account, user lookup, match read side");
    let player = device_player(&env.client, &env.base, "player").await;

    let by_body = json_body(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"ids": [player.uid], "usernames": [player.username]}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/user",
    )
    .await;
    let users = by_body["users"].as_array().expect("a user list").clone();
    assert_eq!(users.len(), 2, "one entry per requested token: {users:?}");
    assert!(
        users.iter().all(|user| user["id"] == player.uid),
        "both tokens name this account: {users:?}"
    );

    let only_ids = json_body(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"ids": [player.uid]}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/user by id alone",
    )
    .await;
    let found = only_ids["users"].as_array().expect("a user list").clone();
    assert_eq!(found.len(), 1, "the id resolves one account: {found:?}");
    assert_eq!(found[0]["id"], player.uid);
    assert_eq!(found[0]["username"], player.username);
    let nothing = json_body(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/user with an empty body",
    )
    .await;
    assert_eq!(
        nothing["users"].as_array().map(Vec::len),
        Some(0),
        "nothing requested is nothing returned: {nothing}"
    );
    refusal(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"ids": ["not-a-uuid"]}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "POST /v2/user with a malformed id",
    )
    .await;

    let updated = json_body(
        env.client
            .put(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .json(&json!({
                "display_name": "Gap Runner",
                "avatar_url": "https://example.test/avatar.png",
                "lang_tag": "en-GB",
                "location": "Deventer",
                "timezone": "Europe/Amsterdam",
                "metadata": "{\"gap\":true}",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/account",
    )
    .await;
    assert_eq!(updated["display_name"], "Gap Runner");
    assert_eq!(updated["timezone"], "Europe/Amsterdam");
    assert_eq!(json_string(&updated["metadata"]), json!({"gap": true}));
    assert_eq!(updated["user_id"], player.uid);
    assert_eq!(
        updated["device_accounts"].as_array().map(Vec::len),
        Some(1),
        "the device that signed in is listed"
    );

    let custom = format!("gap-custom-{}", letters_suffix());
    expect(
        env.client
            .put(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"custom_id": custom}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "claiming a custom id",
    )
    .await;
    let other = device_player(&env.client, &env.base, "player2").await;
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/account", env.base))
                .bearer_auth(&other.token)
                .json(&json!({"custom_id": custom}))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claiming a taken custom id",
        )
        .await,
        "custom id already in use by another account"
    );
    refusal(
        env.client
            .put(format!("{}/v2/account", env.base))
            .bearer_auth(&other.token)
            .json(&json!({"metadata": "{not json"}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "account metadata that does not parse",
    )
    .await;

    let matches = json_body(
        env.client
            .get(format!("{}/v2/match", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/match",
    )
    .await;
    assert!(
        matches["matches"].is_null() || matches["matches"].as_array() == Some(&Vec::new()),
        "the empty registry is omitted, not invented: {matches}"
    );
    refusal(
        env.client
            .get(format!("{}/v2/match?min_size=-1", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "GET /v2/match rejects a negative size",
    )
    .await;
    let stats = json_body(
        env.client
            .get(format!("{}/v2/matchmaker/stats", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/matchmaker/stats",
    )
    .await;
    assert_eq!(stats["ticket_count"], 0);

    let deleted = json_body(
        env.client
            .get(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/account before deletion",
    )
    .await;
    assert_eq!(deleted["display_name"], "Gap Runner");
    expect(
        env.client
            .delete(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/account",
    )
    .await;
    let rows = count(
        env.db(),
        "users",
        Some("id = <uuid>$user"),
        &[("user", json!(player.uid))],
    )
    .await;
    assert_eq!(rows, 0, "the account row is gone");
    let tombstones = count(
        env.db(),
        "user_tombstone",
        Some("(user_id = <uuid>$user OR user_id = $user)"),
        &[("user", json!(player.uid))],
    )
    .await;
    assert_eq!(tombstones, 1, "the tombstone remembers the deletion");
}
