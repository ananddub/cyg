use super::*;

pub async fn tournaments(client: &Client, base: &str, console: &str, pool: &Datastore) {
    eprintln!("e2e: tournaments");
    let now = Utc::now();
    let stamp = |offset: i64| (now + ChronoDuration::seconds(offset)).to_rfc3339();
    let suffix = letters_suffix();
    let joined_id = format!("e2e-tournament-joined-{suffix}");
    let open_id = format!("e2e-tournament-open-{suffix}");
    let ended_id = format!("e2e-tournament-ended-{suffix}");
    let authoritative_id = format!("e2e-tournament-authoritative-{suffix}");
    let plain_id = format!("e2e-leaderboard-{suffix}");

    let alpha = device_player(client, base, "alpha").await;
    let bravo = device_player(client, base, "bravo").await;
    let charlie = device_player(client, base, "charlie").await;

    refusal(
        client
            .post(format!("{base}/v2/console/tournament"))
            .json(&json!({ "id": "anonymous", "duration": 60 }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "define a tournament without a console token",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/tournament"))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "browse tournaments without a session",
    )
    .await;

    for (body, expected) in [
        (json!({ "duration": 60 }), "Tournament ID must be provided."),
        (
            json!({ "id": format!("e2e-bad-duration-{suffix}") }),
            "A tournament needs a duration of at least one second.",
        ),
        (
            json!({
                "id": format!("e2e-bad-size-{suffix}"),
                "duration": 60,
                "max_size": 0
            }),
            "Both max_size and max_num_score must be positive.",
        ),
        (
            json!({
                "id": format!("e2e-bad-schedule-{suffix}"),
                "duration": 60,
                "reset_schedule": "every minute"
            }),
            "leaderboard reset schedule is not a cron expression",
        ),
    ] {
        let message = refusal(
            client
                .post(format!("{base}/v2/console/tournament"))
                .bearer_auth(console)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an invalid tournament definition",
        )
        .await;
        assert_eq!(message, expected);
    }

    let joined = define_tournament(
        client,
        base,
        console,
        json!({
            "id": joined_id.clone(),
            "title": "E2E Joined Tournament",
            "description": "covered by rest_e2e",
            "duration": 3600,
            "start_time": stamp(-300),
            "join_required": true,
            "max_size": 2,
            "max_num_score": 3,
            "metadata": { "lane": "e2e" }
        }),
    )
    .await;
    assert_eq!(joined["id"], joined_id);
    assert_eq!(joined["title"], "E2E Joined Tournament");
    assert_eq!(joined["duration"], 3600);
    assert_eq!(joined["join_required"], true);
    assert_eq!(joined["max_size"], 2);
    assert_eq!(joined["max_num_score"], 3);
    assert_eq!(joined["can_enter"], true);
    assert!(joined["metadata"].as_str().unwrap().contains("lane"));
    assert!(joined["start_time"].is_string());
    assert!(joined.get("size").is_none(), "no entrant prints no size");
    assert!(
        joined.get("end_time").is_none(),
        "an open-ended board has no end time on the wire"
    );

    define_tournament(
        client,
        base,
        console,
        json!({
            "id": open_id.clone(),
            "title": "E2E Open Tournament",
            "duration": 1800,
            "start_time": stamp(-240),
            "max_size": 2,
            "max_num_score": 10
        }),
    )
    .await;
    define_tournament(
        client,
        base,
        console,
        json!({
            "id": authoritative_id.clone(),
            "title": "E2E Authoritative Tournament",
            "duration": 1800,
            "start_time": stamp(-180),
            "authoritative": true
        }),
    )
    .await;
    define_tournament(
        client,
        base,
        console,
        json!({
            "id": ended_id.clone(),
            "title": "E2E Ended Tournament",
            "duration": 3600,
            "start_time": stamp(-7200),
            "end_time": stamp(-3600),
            "join_required": true
        }),
    )
    .await;

    seed_board(pool, &plain_id).await;

    let duplicate = refusal(
        client
            .post(format!("{base}/v2/console/tournament"))
            .bearer_auth(console)
            .json(&json!({ "id": joined_id.clone(), "duration": 60 }))
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "a duplicate tournament id",
    )
    .await;
    assert_eq!(duplicate, "Tournament id is already taken.");

    let page = json_body(
        client
            .get(format!("{base}/v2/tournament?limit=10"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse tournaments",
    )
    .await;
    assert_eq!(
        tournament_ids(&page),
        vec![joined_id.clone(), open_id.clone(), authoritative_id.clone()]
    );
    let listed = page["tournaments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == joined_id)
        .expect("the joined tournament is listed");
    assert_eq!(listed["can_enter"], true);
    assert_eq!(listed["join_required"], true);
    assert!(listed.get("size").is_none());

    let first = json_body(
        client
            .get(format!("{base}/v2/tournament?limit=1"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse tournaments, first page",
    )
    .await;
    assert_eq!(tournament_ids(&first), vec![joined_id.clone()]);
    let cursor = first["cursor"]
        .as_str()
        .expect("a full page carries a cursor")
        .to_owned();
    assert!(!cursor.is_empty());
    let second = json_body(
        client
            .get(format!("{base}/v2/tournament?limit=1&cursor={cursor}"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse tournaments, second page",
    )
    .await;
    assert_eq!(tournament_ids(&second), vec![open_id.clone()]);

    let closed = json_body(
        client
            .get(format!("{base}/v2/tournament?end_time={}", now.timestamp()))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse ended tournaments",
    )
    .await;
    assert_eq!(tournament_ids(&closed), vec![ended_id.clone()]);

    let empty = json_body(
        client
            .get(format!("{base}/v2/tournament?category_start=2"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse outside the category",
    )
    .await;
    assert!(tournament_ids(&empty).is_empty());

    for (query, expected) in [
        (
            "start_time=1000&end_time=500",
            "Tournament end time must be greater than start time.",
        ),
        ("limit=101", "Limit must be between 1 and 100."),
        (
            "category_start=5&category_end=1",
            "Tournament category end must be greater than category start.",
        ),
    ] {
        let message = refusal(
            client
                .get(format!("{base}/v2/tournament?{query}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an invalid tournament query",
        )
        .await;
        assert_eq!(message, expected);
    }

    for id in ["does-not-exist", plain_id.as_str()] {
        let message = refusal(
            client
                .get(format!("{base}/v2/tournament/{id}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a non-tournament board",
        )
        .await;
        assert_eq!(message, "Tournament not found.");
    }

    let joined_url = format!("{base}/v2/tournament/{joined_id}");
    expect(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "alpha joins",
    )
    .await;
    let size = board_size(pool, &joined_id).await;
    assert_eq!(size, 1);
    expect(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "alpha joins twice",
    )
    .await;
    let size: i64 = board_size(pool, &joined_id).await;
    assert_eq!(size, 1, "a second join is the same entrant");

    let joined_row = row(
        pool,
        "SELECT num_score, username, expiry_time FROM leaderboard_record
         WHERE leaderboard_id = $board AND (owner_id = <uuid>$owner OR owner_id = $owner) LIMIT 1",
        &[("board", json!(joined_id)), ("owner", json!(alpha.uid))],
    )
    .await
    .expect("the join row exists");
    assert_eq!(joined_row["num_score"], 0);
    assert_eq!(joined_row["username"], json!(alpha.username));
    assert_eq!(
        joined_row["expiry_time"],
        json!(engine_api::db::time::EPOCH),
        "an open-ended round stores the epoch window"
    );

    let message = refusal(
        client
            .post(&joined_url)
            .bearer_auth(&bravo.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write before joining",
    )
    .await;
    assert_eq!(
        message,
        "Must join tournament before attempting to write value."
    );
    expect(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&bravo.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "bravo joins",
    )
    .await;
    let size: i64 = board_size(pool, &joined_id).await;
    assert_eq!(size, 2);
    let message = refusal(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&charlie.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "join a full tournament",
    )
    .await;
    assert_eq!(
        message,
        "Tournament cannot be joined as it has reached its max size."
    );

    let first = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 10, "metadata": "{\"mood\":\"happy\"}" }),
    )
    .await;
    assert_eq!(first["score"], "10");
    assert_eq!(first["num_score"], 1);
    assert_eq!(first["owner_id"], alpha.uid);
    assert_eq!(first["username"], alpha.username);
    assert_eq!(first["rank"], "1");
    assert!(first["metadata"].as_str().unwrap().contains("happy"));
    assert!(
        first.get("expiry_time").is_none(),
        "the epoch window prints no expiry"
    );

    let second = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 5 }),
    )
    .await;
    assert_eq!(second["score"], "10", "best keeps the higher score");
    assert_eq!(second["num_score"], 2);

    let third = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 3, "operator": 2 }),
    )
    .await;
    assert_eq!(third["score"], "3", "operator 2 replaces the score");
    assert_eq!(third["num_score"], 3);

    let capped = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 4 }),
    )
    .await;
    assert_eq!(capped["score"], "3");
    assert_eq!(
        capped["num_score"], 3,
        "the attempt cap leaves the record untouched"
    );

    let bravo_record = write_record(
        client,
        base,
        &bravo.token,
        &joined_id,
        json!({ "score": 7, "subscore": 4 }),
    )
    .await;
    assert_eq!(bravo_record["score"], "7");
    assert_eq!(bravo_record["subscore"], "4");
    assert_eq!(bravo_record["rank"], "1");
    assert_eq!(bravo_record["num_score"], 1);

    let stored = row(
        pool,
        "SELECT score, num_score, username FROM leaderboard_record
         WHERE leaderboard_id = $board AND (owner_id = <uuid>$owner OR owner_id = $owner) LIMIT 1",
        &[("board", json!(joined_id)), ("owner", json!(alpha.uid))],
    )
    .await
    .expect("alpha's record is stored");
    assert_eq!(stored["score"], 3);
    assert_eq!(stored["num_score"], 3);
    assert_eq!(stored["username"], json!(alpha.username));
    let size: i64 = board_size(pool, &joined_id).await;
    assert_eq!(size, 2, "writes never move the join counter");

    let listing = json_body(
        client
            .get(&joined_url)
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list tournament records",
    )
    .await;
    let records = listing["records"].as_array().expect("records page");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["owner_id"], bravo.uid);
    assert_eq!(records[0]["score"], "7");
    assert_eq!(records[0]["rank"], "1");
    assert_eq!(records[1]["owner_id"], alpha.uid);
    assert_eq!(records[1]["score"], "3");
    assert_eq!(records[1]["rank"], "2");
    assert_eq!(listing["rank_count"], "2");

    let owners = json_body(
        client
            .get(format!("{joined_url}?owner_ids={}", alpha.uid))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list named owner records",
    )
    .await;
    assert_eq!(
        owners["records"].as_array().map(Vec::len),
        Some(2),
        "without a handed-back cursor the round is still paged"
    );
    let own = owners["owner_records"].as_array().expect("owner records");
    assert_eq!(own.len(), 1);
    assert_eq!(own[0]["owner_id"], alpha.uid);
    assert_eq!(own[0]["score"], "3");

    let paged = json_body(
        client
            .get(format!("{joined_url}?limit=1"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "one record of the round",
    )
    .await;
    let cursor = paged["next_cursor"]
        .as_str()
        .expect("a truncated page carries a cursor")
        .to_owned();
    let only_owners = json_body(
        client
            .get(format!(
                "{joined_url}?owner_ids={}&cursor={cursor}",
                alpha.uid
            ))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "owners with a cursor",
    )
    .await;
    assert!(
        only_owners.get("records").is_none(),
        "owners with a cursor pages nothing"
    );
    let own = only_owners["owner_records"]
        .as_array()
        .expect("owner records");
    assert_eq!(own.len(), 1);
    assert_eq!(own[0]["owner_id"], alpha.uid);

    let around = json_body(
        client
            .get(format!("{joined_url}/owner/{}", alpha.uid))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "haystack around the owner",
    )
    .await;
    let window = around["records"].as_array().expect("haystack records");
    assert_eq!(window.len(), 2);
    assert!(window.iter().any(|row| row["owner_id"] == alpha.uid));

    for (query, expected) in [
        (
            "limit=101",
            "Invalid limit - limit must be between 1 and 100.",
        ),
        ("owner_ids=nope", "One or more owner IDs are invalid."),
    ] {
        let message = refusal(
            client
                .get(format!("{joined_url}?{query}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an invalid records query",
        )
        .await;
        assert_eq!(message, expected);
    }

    let ended_expectations = [
        (
            client
                .get(format!("{base}/v2/tournament/{ended_id}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "Tournament has ended.",
            "list the records of an ended tournament",
        ),
        (
            client
                .post(format!("{base}/v2/tournament/{ended_id}"))
                .bearer_auth(&alpha.token)
                .json(&json!({ "score": 1 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "Tournament not found or has ended.",
            "write to an ended tournament",
        ),
        (
            client
                .post(format!("{base}/v2/tournament/{ended_id}/join"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "Tournament is not active and cannot accept new joins.",
            "join an ended tournament",
        ),
    ];
    for (response, status, expected, what) in ended_expectations {
        let message = refusal(response, status, what).await;
        assert_eq!(message, expected);
    }
    let closed_window = json_body(
        client
            .get(format!(
                "{base}/v2/tournament/{ended_id}/owner/{}",
                alpha.uid
            ))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "haystack on an ended tournament",
    )
    .await;
    assert!(closed_window.get("records").is_none());

    let message = refusal(
        client
            .post(format!("{base}/v2/tournament/{authoritative_id}"))
            .bearer_auth(&alpha.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "write to an authoritative tournament",
    )
    .await;
    assert_eq!(
        message,
        "Tournament only allows authoritative score submissions."
    );

    let open_url = format!("{base}/v2/tournament/{open_id}");
    let open_first = write_record(
        client,
        base,
        &alpha.token,
        &open_id,
        json!({ "score": 100 }),
    )
    .await;
    assert_eq!(open_first["score"], "100");
    assert_eq!(open_first["num_score"], 1);
    let open_second = write_record(
        client,
        base,
        &charlie.token,
        &open_id,
        json!({ "score": 50 }),
    )
    .await;
    assert_eq!(open_second["num_score"], 1);
    let size: i64 = board_size(pool, &open_id).await;
    assert_eq!(size, 2);

    let better = write_record(client, base, &alpha.token, &open_id, json!({ "score": 90 })).await;
    assert_eq!(better["score"], "100");
    assert_eq!(better["num_score"], 2);
    let incremented = write_record(
        client,
        base,
        &alpha.token,
        &open_id,
        json!({ "score": 5, "operator": 3 }),
    )
    .await;
    assert_eq!(incremented["score"], "105");
    assert_eq!(incremented["num_score"], 3);

    let message = refusal(
        client
            .post(&open_url)
            .bearer_auth(&bravo.token)
            .json(&json!({ "score": 9 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write to a full open tournament",
    )
    .await;
    assert_eq!(message, "Tournament has reached max size.");
    let records = count(
        pool,
        "leaderboard_record",
        Some("leaderboard_id = $board"),
        &[("board", json!(open_id))],
    )
    .await;
    let size = board_size(pool, &open_id).await;
    assert_eq!((records, size), (2, 2), "the refused insert is rolled back");

    expect(
        client
            .delete(&open_url)
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "alpha deletes their record",
    )
    .await;
    let records = count(
        pool,
        "leaderboard_record",
        Some("leaderboard_id = $board"),
        &[("board", json!(open_id))],
    )
    .await;
    let size = board_size(pool, &open_id).await;
    assert_eq!((records, size), (1, 1));
    let reentered = write_record(client, base, &alpha.token, &open_id, json!({ "score": 7 })).await;
    assert_eq!(reentered["score"], "7");
    assert_eq!(reentered["num_score"], 1);
    let size: i64 = board_size(pool, &open_id).await;
    assert_eq!(size, 2);
    expect(
        client
            .delete(&open_url)
            .bearer_auth(&bravo.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a recordless delete is a no-op",
    )
    .await;
    let size: i64 = board_size(pool, &open_id).await;
    assert_eq!(size, 2, "removing nothing releases nothing");

    let updated = json_body(
        client
            .put(format!("{base}/v2/console/tournament/{joined_id}"))
            .bearer_auth(console)
            .json(&json!({
                "title": "E2E Joined Tournament (renamed)",
                "max_num_score": 5
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "rename a tournament",
    )
    .await;
    assert_eq!(updated["title"], "E2E Joined Tournament (renamed)");
    assert_eq!(updated["duration"], 3600);
    assert_eq!(updated["max_num_score"], 5);
    let message = refusal(
        client
            .put(format!("{base}/v2/console/tournament/{joined_id}"))
            .bearer_auth(console)
            .json(&json!({ "duration": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "drop a tournament's duration",
    )
    .await;
    assert_eq!(
        message,
        "A tournament needs a duration of at least one second."
    );
    let message = refusal(
        client
            .put(format!("{base}/v2/console/tournament/{plain_id}"))
            .bearer_auth(console)
            .json(&json!({ "title": "not a tournament" }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "update a plain leaderboard as a tournament",
    )
    .await;
    assert_eq!(message, "Tournament not found.");

    expect(
        client
            .delete(format!("{base}/v2/console/tournament/{ended_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the ended tournament",
    )
    .await;
    expect(
        client
            .delete(format!("{base}/v2/console/tournament/{joined_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the joined tournament",
    )
    .await;
    let remaining = count(
        pool,
        "leaderboard_record",
        Some("leaderboard_id = $board"),
        &[("board", json!(joined_id))],
    )
    .await;
    assert_eq!(remaining, 0, "the board's records cascade with it");
    let message = refusal(
        client
            .get(&joined_url)
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read a deleted tournament",
    )
    .await;
    assert_eq!(message, "Tournament not found.");
    let message = refusal(
        client
            .delete(format!("{base}/v2/console/tournament/{plain_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete a plain leaderboard as a tournament",
    )
    .await;
    assert_eq!(message, "Tournament not found.");

    for id in [&open_id, &authoritative_id] {
        expect(
            client
                .delete(format!("{base}/v2/console/tournament/{id}"))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "delete a tournament",
        )
        .await;
    }
    let page = json_body(
        client
            .get(format!("{base}/v2/tournament"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse after deletions",
    )
    .await;
    assert!(tournament_ids(&page).is_empty());

    expect(
        client
            .delete(format!("{base}/v2/console/leaderboard/{plain_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the plain leaderboard",
    )
    .await;

    for (action, wanted) in [
        ("tournament.create", 4),
        ("tournament.update", 1),
        ("tournament.delete", 4),
    ] {
        let got = count(
            pool,
            "console_audit_log",
            Some("action = $action"),
            &[("action", json!(action))],
        )
        .await;
        assert_eq!(got, wanted, "{action} audit rows");
    }
}
