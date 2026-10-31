use super::*;

pub async fn account_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: account, wallet and session");
    let player = device_player(client, base, "acct").await;
    let other = device_player(client, base, "acctb").await;

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "account without a token",
        )
        .await,
        "Auth token required"
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .bearer_auth("not-a-jwt")
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "account with an unparsable token",
        )
        .await,
        "Auth token invalid"
    );

    let account = json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the account",
    )
    .await;
    assert_eq!(account["user_id"], player.uid);
    assert_eq!(account["username"], player.username);
    assert_eq!(account["lang_tag"], "en");
    assert_eq!(account["metadata"], json!({}));
    assert_eq!(account["wallet"], json!({}));
    assert_eq!(account["edge_count"], 0);
    assert!(account["display_name"].is_null());
    assert!(account["create_time"].as_str().unwrap().ends_with('Z'));
    let devices = account["device_accounts"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["id"], player.device_id);
    assert_eq!(devices[0]["provider"], "");
    assert!(devices[0]["vars"]["push_token_ios"].is_string());

    let renamed = format!("renamed{}", letters_suffix());
    let updated = json_body(
        client
            .put(format!("{base}/v2/account"))
            .bearer_auth(&player.token)
            .json(&json!({
                "display_name": "E2E Hero",
                "avatar_url": "https://example.test/hero.png",
                "lang_tag": "fr",
                "location": "Test Range",
                "timezone": "UTC",
                "metadata": "{\"tier\":\"gold\"}",
                "username": renamed,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "update the account",
    )
    .await;
    assert_eq!(updated["username"], renamed);
    assert_eq!(updated["display_name"], "E2E Hero");
    assert_eq!(updated["avatar_url"], "https://example.test/hero.png");
    assert_eq!(updated["lang_tag"], "fr");
    assert_eq!(updated["location"], "Test Range");
    assert_eq!(updated["timezone"], "UTC");
    assert_eq!(updated["metadata"], json!({ "tier": "gold" }));
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/account"))
                .bearer_auth(&player.token)
                .json(&json!({ "metadata": "{broken" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "account metadata must parse",
        )
        .await,
        "invalid account metadata"
    );

    let shared_custom = format!("shared-{}", letters_suffix());
    let claimed = json_body(
        client
            .put(format!("{base}/v2/account"))
            .bearer_auth(&player.token)
            .json(&json!({ "custom_id": shared_custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "claim a custom id",
    )
    .await;
    assert_eq!(claimed["custom_id"], shared_custom);
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/account"))
                .bearer_auth(&other.token)
                .json(&json!({ "custom_id": shared_custom }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim a taken custom id",
        )
        .await,
        "custom id already in use by another account"
    );

    let wallet = json_body(
        client
            .get(format!("{base}/v2/account/wallet"))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the wallet",
    )
    .await;
    assert_eq!(wallet["wallet"], json!({}));
    assert!(wallet["ledger_items"].as_array().unwrap().is_empty());
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "wallet_items": { "gold": 10 } }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a player cannot mint currency",
        )
        .await,
        "Currency changes require a console write; a player may only merge ledger metadata."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an empty wallet write",
        )
        .await,
        "There is nothing to write: send ledger metadata to merge."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "ledger_items": [{ "id": "not-a-uuid", "metadata": {} }] }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a malformed ledger id",
        )
        .await,
        "Invalid ledger item id."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "ledger_items": [{ "id": Uuid::new_v4().to_string(), "metadata": {} }] }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a ledger entry that does not exist",
        )
        .await,
        "That ledger entry does not belong to this account."
    );

    let ledger_id = Uuid::new_v4();
    seed_ledger(pool, ledger_id, &player.uid, json!({"gold": 25})).await;
    let merged = json_body(
        client
            .post(format!("{base}/v2/account/wallet"))
            .bearer_auth(&player.token)
            .json(&json!({ "ledger_items": [{ "id": ledger_id.to_string(), "metadata": { "note": "merged" } }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "merge ledger metadata",
    )
    .await;
    let row = merged["ledger_items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == ledger_id.to_string())
        .expect("the merged ledger row is returned");
    assert_eq!(row["metadata"]["note"], "merged");
    assert_eq!(merged["wallet"], json!({}));

    let second_ledger = Uuid::new_v4();
    seed_ledger(pool, second_ledger, &player.uid, json!({"gems": 5})).await;
    json_body(
        client
            .put(format!("{base}/v2/account/wallet"))
            .bearer_auth(&player.token)
            .json(&json!({ "ledger_items": [{ "id": second_ledger.to_string(), "metadata": { "note": "put" } }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the PUT wallet variant merges too",
    )
    .await;
    let foreign_ledger = Uuid::new_v4();
    seed_ledger(pool, foreign_ledger, &other.uid, json!({})).await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "ledger_items": [{ "id": foreign_ledger.to_string(), "metadata": {} }] }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "merge another account's ledger entry",
        )
        .await,
        "That ledger entry does not belong to this account."
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .json(&json!({ "token": player.refresh_token }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "refresh without the server key",
        )
        .await,
        "Server key required"
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "token": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "refresh without a token",
        )
        .await,
        "Refresh token is required."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "token": "not-a-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "refresh with an unparsable token",
        )
        .await,
        "Refresh token invalid or expired."
    );
    let refreshed = json_body(
        client
            .post(format!("{base}/v2/account/session/refresh"))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "token": player.refresh_token, "vars": { "platform": "refreshed" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "refresh the session",
    )
    .await;
    assert!(refreshed.get("created").is_none());
    assert_eq!(jwt_uid(&refreshed["token"]), player.uid);
    let refreshed_token = refreshed["token"].as_str().unwrap().to_owned();
    let refreshed_refresh = refreshed["refresh_token"].as_str().unwrap().to_owned();
    json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&refreshed_token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the account with the refreshed token",
    )
    .await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/session/logout"))
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "logout without a token",
        )
        .await,
        "Auth token required"
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/session/logout"))
                .bearer_auth(&refreshed_token)
                .json(&json!({ "token": "not-a-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "logout naming an unparsable token",
        )
        .await,
        "Session token invalid."
    );
    let logged_out = json_body(
        client
            .post(format!("{base}/v2/session/logout"))
            .bearer_auth(&refreshed_token)
            .json(&json!({ "token": refreshed_token, "refresh_token": refreshed_refresh }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "log out the pair",
    )
    .await;
    assert_eq!(logged_out, json!({}));
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .bearer_auth(&refreshed_token)
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a logged-out token is revoked",
        )
        .await,
        "Auth token invalid"
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "token": refreshed_refresh }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a logged-out refresh token is revoked",
        )
        .await,
        "Refresh token invalid or expired."
    );

    let doomed = device_player(client, base, "del").await;
    let deleted = json_body(
        client
            .delete(format!("{base}/v2/account"))
            .bearer_auth(&doomed.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the account",
    )
    .await;
    assert_eq!(deleted, json!({}));
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .bearer_auth(&doomed.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "read a deleted account",
        )
        .await,
        "Entity not found."
    );
    let tombstoned = count(
        pool,
        "user_tombstone",
        Some("(user_id = <uuid>$u OR user_id = $u)"),
        &[("u", json!(doomed.uid))],
    )
    .await;
    assert_eq!(tombstoned, 1);
    let remaining = count(
        pool,
        "type::record('users', <uuid>$u)",
        None,
        &[("u", json!(doomed.uid))],
    )
    .await;
    assert_eq!(remaining, 0);
}
