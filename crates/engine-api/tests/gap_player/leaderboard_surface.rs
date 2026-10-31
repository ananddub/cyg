use super::*;

pub async fn leaderboard_surface(env: &Harness) {
    eprintln!("gap: leaderboard records, list, haystack and delete");
    let suffix = letters_suffix();
    let board = format!("e2e_gap_board_{suffix}");
    let auth_board = format!("e2e_gap_auth_{suffix}");
    let round = format!("e2e_gap_round_{suffix}");
    seed_board(env, &board, 0, 0, 1, false).await;
    seed_board(env, &auth_board, 0, 0, 1, true).await;
    seed_board(env, &round, 0, 3600, 1, false).await;

    let lead = device_player(&env.client, &env.base, "lbd").await;
    let rival = device_player(&env.client, &env.base, "lbr").await;

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{}", env.base, board))
                .bearer_auth(&lead.token)
                .json(&Value::Null)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a body the gateway reads as no record at all",
        )
        .await,
        "Invalid input, record score value is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{}", env.base, board))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": "not-a-number" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a score that is neither int nor numeric string",
        )
        .await,
        "Invalid input, record score value is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{}", env.base, board))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10, "metadata": "not json" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "metadata that is not JSON",
        )
        .await,
        "Metadata value must be JSON, if provided."
    );
    let ghost = format!("e2e_gap_ghost_{suffix}");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{ghost}", env.base))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a record for a board that does not exist",
        )
        .await,
        "Leaderboard not found."
    );

    let first = json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "score": 500,
                "subscore": 7,
                "metadata": "{\"a\":1}",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/leaderboard/{leaderboardId} (first write)",
    )
    .await;
    assert_eq!(first["leaderboard_id"], board);
    assert_eq!(first["owner_id"], lead.uid);
    assert_eq!(first["score"], "500", "the int64 comes back as a string");
    assert_eq!(first["subscore"], "7");
    assert_eq!(json_string(&first["metadata"]), json!({"a": 1}));
    assert_eq!(first["rank"], "1", "with ranks on, the write names the standing");

    let ignored = json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({ "score": 300 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the default BEST operator ignoring a lower score",
    )
    .await;
    assert_eq!(ignored["score"], "500", "best stays best");
    for (body, wanted) in [
        (json!({ "score": 700, "operator": 2 }), "700"),
        (json!({ "score": 50, "operator": 3 }), "750"),
        (json!({ "score": 100, "operator": 4 }), "650"),
    ] {
        let written = json_body(
            env.client
                .post(format!("{}/v2/leaderboard/{board}", env.base))
                .bearer_auth(&lead.token)
                .json(&body)
                .send()
            .await
            .unwrap(),
            StatusCode::OK,
            &format!("the {:?} operator", body["operator"]),
        )
        .await;
        assert_eq!(written["score"], wanted, "{body}");
    }
    json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&rival.token)
            .json(&json!({ "score": 400 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the rival scoring in",
    )
    .await;

    let page = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}?limit=1", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId}?limit=1",
    )
    .await;
    let rows = page["records"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 1, "{page}");
    assert_eq!(rows[0]["score"], "650", "the board reads in score order");
    assert_ne!(page["next_cursor"], json!(""), "a page that cut records off hands back a cursor");
    let wide = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}?limit=10", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId} (full page)",
    )
    .await;
    assert_eq!(wide["records"].as_array().map(Vec::len), Some(2), "{wide}");
    assert_eq!(wide["rank_count"], "2");
    let owners = json_body(
        env.client
            .get(format!(
                "{}/v2/leaderboard/{board}?owner_ids={}",
                env.base,
                format!("{},{}", rival.uid, lead.uid)
            ))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId}?owner_ids= (batch)",
    )
    .await;
    assert_eq!(owners["owner_records"].as_array().map(Vec::len), Some(2), "{owners}");
    assert!(owners.get("records").is_none(), "an un-paged owner batch lists no page");
    for (label, query, message) in [
        ("limit=0", "limit=0", "Invalid limit - limit must be between 1 and 1000."),
        ("limit=abc", "limit=abc", "Invalid limit - limit must be between 1 and 1000."),
        ("a bad owner id", "owner_ids=not-a-uuid", "One or more owner IDs are invalid."),
        ("a bad expiry", "expiry=soon", "Invalid expiry - expiry must be a UNIX timestamp."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .get(format!("{}/v2/leaderboard/{board}?{query}", env.base))
                    .bearer_auth(&lead.token)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                &format!("the listing refusing {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }

    let hay = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}/owner/{}", env.base, rival.uid))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/leaderboard/{leaderboardId}/owner/{ownerId}",
    )
    .await;
    assert_eq!(
        hay["records"]
            .as_array()
            .map(|rows| rows.iter().any(|r| r["owner_id"] == rival.uid))
            .unwrap_or(false),
        true,
        "the haystack is anchored on the owner: {hay}"
    );
    assert_eq!(hay["rank_count"], "2");
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/leaderboard/{board}/owner/not-a-uuid", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a haystack for an account that is not a uuid",
        )
        .await,
        "Invalid owner ID provided."
    );
    assert_eq!(
        json_body(
            env.client
                .get(format!("{}/v2/leaderboard/{board}/owner/{}", env.base, Uuid::new_v4()))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "a haystack for an owner with no record",
        )
        .await,
        json!({}),
        "the empty page prints nothing at all"
    );

    expect(
        env.client
            .delete(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/leaderboard/{leaderboardId}",
    )
    .await;
    assert_eq!(
        json_body(
            env.client
                .get(format!("{}/v2/leaderboard/{board}/owner/{}", env.base, lead.uid))
                .bearer_auth(&lead.token)
                .send()
                .await
            .unwrap(),
            StatusCode::OK,
            "the deleted owner's haystack",
        )
        .await,
        json!({}),
        "the caller's record is gone"
    );
    let survivors = json_body(
        env.client
            .get(format!("{}/v2/leaderboard/{board}?limit=10", env.base))
            .bearer_auth(&rival.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the board after the record delete",
    )
    .await;
    assert_eq!(survivors["records"].as_array().map(Vec::len), Some(1), "{survivors}");
    assert_eq!(survivors["records"][0]["owner_id"], rival.uid);
    for (label, call) in [
        (
            "writing to an authoritative board",
            env.client
                .post(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
        ),
        (
            "deleting from an authoritative board",
            env.client
                .delete(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
        ),
    ] {
        refusal(call, StatusCode::FORBIDDEN, label).await;
    }
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "the authoritative write",
        )
        .await,
        "Leaderboard only allows authoritative score submissions."
    );
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/leaderboard/{auth_board}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "the authoritative delete",
        )
        .await,
        "Leaderboard only allows authoritative score deletions."
    );
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/leaderboard/{round}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting a tournament record through the leaderboard route",
        )
        .await,
        "Leaderboard not found."
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/leaderboard/{ghost}", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "listing a board that does not exist",
        )
        .await,
        "Leaderboard not found."
    );
}
