use super::*;

pub async fn tournament_surface(env: &Harness, console: &str) {
    eprintln!("gap: tournament round, join, scores and standing");
    let suffix = letters_suffix();
    let joined_id = format!("e2e_gap_tjoin_{suffix}");
    let closed_id = format!("e2e_gap_tclosed_{suffix}");
    let open_id = format!("e2e_gap_topen_{suffix}");
    let plain_id = format!("e2e_gap_tplain_{suffix}");
    let now = Utc::now();
    let stamp = |offset: i64| (now + ChronoDuration::seconds(offset)).to_rfc3339();

    for (id, join_required, start) in [
        (joined_id.as_str(), true, stamp(-300)),
        (closed_id.as_str(), true, stamp(-7_200)),
        (open_id.as_str(), false, stamp(-300)),
    ] {
        json_body(
            env.client
                .post(format!("{}/v2/console/tournament", env.base))
                .bearer_auth(console)
                .json(&json!({
                    "id": id,
                    "title": format!("Gap Round {id}"),
                    "duration": 3_600,
                    "start_time": start,
                    "join_required": join_required,
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            &format!("define the round {id}"),
        )
        .await;
    }
    seed_board(env, &plain_id, 0, 0, 1, false).await;

    let entrant = device_player(&env.client, &env.base, "tent").await;
    let other = device_player(&env.client, &env.base, "toth").await;

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{joined_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&Value::Null)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a body that names no record",
        )
        .await,
        "Invalid input, record score value is required."
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{joined_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({ "score": 120 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "scoring before joining",
        )
        .await,
        "Must join tournament before attempting to write value."
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{closed_id}/join", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "joining a round that already closed",
        )
        .await,
        "Tournament is not active and cannot accept new joins."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{closed_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "scoring into a closed round",
        )
        .await,
        "Tournament is not active and cannot accept new scores."
    );

    expect(
        env.client
            .post(format!("{}/v2/tournament/{open_id}/join", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/tournament/{tournamentId}/join on a round that does not ask for one",
    )
    .await;
    let no_op = count(
        env.db(),
        "leaderboard_record",
        Some("leaderboard_id = $board AND (owner_id = <uuid>$owner OR owner_id = $owner)"),
        &[("board", json!(open_id)), ("owner", json!(entrant.uid))],
    )
    .await;
    assert_eq!(no_op, 0, "the no-op join wrote nothing");

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{joined_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a body that names no record",
        )
        .await,
        "Invalid input, record score value is required."
    );
    expect(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}/join", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/tournament/{tournamentId}/join",
    )
    .await;
    let wrote = json_body(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&entrant.token)
            .json(&json!({ "score": 120, "metadata": "{\"lane\":\"gap\"}" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/tournament/{tournamentId}",
    )
    .await;
    assert_eq!(wrote["leaderboard_id"], joined_id);
    assert_eq!(wrote["owner_id"], entrant.uid);
    assert_eq!(wrote["score"], "120", "the int64 comes back as a string");
    assert_eq!(json_string(&wrote["metadata"]), json!({"lane": "gap"}));
    assert!(
        wrote.get("expiry_time").is_none(),
        "an endless round writes no deadline: {wrote}"
    );

    let ending_id = format!("e2e_gap_tend_{suffix}");
    json_body(
        env.client
            .post(format!("{}/v2/console/tournament", env.base))
            .bearer_auth(console)
            .json(&json!({
                "id": &ending_id,
                "title": "Gap Bounded Round",
                "duration": 3_600,
                "start_time": stamp(-300),
                "end_time": stamp(1_800),
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "define a round that ends",
    )
    .await;
    let bounded = json_body(
        env.client
            .post(format!("{}/v2/tournament/{ending_id}", env.base))
            .bearer_auth(&entrant.token)
            .json(&json!({ "score": 5 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "scoring into the round that ends",
    )
    .await;
    let deadline = DateTime::parse_from_rfc3339(
        bounded["expiry_time"]
            .as_str()
            .expect("the record names its round"),
    )
    .expect("the deadline is an RFC 3339 timestamp");
    assert_eq!(
        deadline.timestamp(),
        (now + ChronoDuration::seconds(1_800)).timestamp(),
        "the record sits in the round's own window: {bounded}"
    );

    let replaced = json_body(
        env.client
            .put(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&entrant.token)
            .json(&json!({ "score": 90, "operator": 2 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/tournament/{tournamentId}",
    )
    .await;
    assert_eq!(replaced["score"], "90", "SET replaces the round score");

    json_body(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}/join", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the second entrant joining",
    )
    .await;
    json_body(
        env.client
            .post(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&other.token)
            .json(&json!({ "score": 130 }))
            .send()
        .await
        .unwrap(),
        StatusCode::OK,
        "the second entrant scoring",
    )
    .await;

    let page = json_body(
        env.client
            .get(format!("{}/v2/tournament/{joined_id}?limit=5", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/tournament/{tournamentId}",
    )
    .await;
    let rows = page["records"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 2, "{page}");
    assert_eq!(rows[0]["score"], "130", "the round reads in score order");
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/tournament/{joined_id}?limit=0", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the round page refusing limit=0",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );

    let standing = json_body(
        env.client
            .get(format!("{}/v2/tournament/{joined_id}/owner/{}", env.base, other.uid))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/tournament/{tournamentId}/owner/{ownerId}",
    )
    .await;
    assert_eq!(
        standing["records"]
            .as_array()
            .map(|rows| rows.iter().any(|r| r["owner_id"] == other.uid))
            .unwrap_or(false),
        true,
        "the haystack anchors on the entrant: {standing}"
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/tournament/{joined_id}/owner/not-a-uuid", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a standing for an account that is not a uuid",
        )
        .await,
        "Invalid owner ID provided."
    );

    expect(
        env.client
            .delete(format!("{}/v2/tournament/{joined_id}", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/tournament/{tournamentId}",
    )
    .await;
    let after = json_body(
        env.client
            .get(format!("{}/v2/tournament/{joined_id}?limit=5", env.base))
            .bearer_auth(&entrant.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the round after the delete",
    )
    .await;
    assert_eq!(after["records"].as_array().map(Vec::len), Some(1), "{after}");
    assert_eq!(after["records"][0]["owner_id"], entrant.uid);

    for (label, id, status, message) in [
        ("listing a plain board as a round", &plain_id, StatusCode::NOT_FOUND, "Tournament not found."),
        ("writing to a round that does not exist", &format!("e2e_gap_missing_{suffix}"), StatusCode::NOT_FOUND, "Tournament not found."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .get(format!("{}/v2/tournament/{id}", env.base))
                    .bearer_auth(&entrant.token)
                    .send()
                    .await
                    .unwrap(),
                status,
                label,
            )
            .await,
            message,
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{plain_id}", env.base))
                .bearer_auth(&entrant.token)
                .json(&json!({ "score": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "scoring into a plain leaderboard",
        )
        .await,
        "Tournament not found."
    );
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/tournament/{plain_id}", env.base))
                .bearer_auth(&entrant.token)
                .send()
                .await
                .unwrap(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "deleting from a plain leaderboard through the tournament route",
        )
        .await,
        "Error deleting score from tournament."
    );
}
