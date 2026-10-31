use super::*;

pub async fn leaderboard_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: leaderboards");
    let alice = device_player(client, base, "lb-a").await;
    let bob = device_player(client, base, "lb-b").await;
    let charlie = device_player(client, base, "lb-c").await;
    let suffix = letters_suffix();
    let desc_id = format!("e2e-lb-desc-{suffix}");
    let ops_id = format!("e2e-lb-ops-{suffix}");
    let asc_id = format!("e2e-lb-asc-{suffix}");
    let auth_id = format!("e2e-lb-auth-{suffix}");
    let unranked_id = format!("e2e-lb-unranked-{suffix}");
    let tournament_id = format!("e2e-lb-round-{suffix}");

    for (id, sort_order, authoritative, enable_ranks) in [
        (&desc_id, 1_i16, false, true),
        (&ops_id, 1, false, true),
        (&asc_id, 0, false, true),
        (&auth_id, 1, true, true),
        (&unranked_id, 1, false, false),
    ] {
        seed_board_opts(pool, id, sort_order, authoritative, enable_ranks, 0, 0, false).await;
    }
    seed_board_opts(pool, &tournament_id, 1, false, true, 0, 0, true).await;

    let url = |id: &str| format!("{base}/v2/leaderboard/{id}");

    refusal(
        client
            .post(url(&desc_id))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write a record with no body",
    )
    .await;
    let first = api_post(
        client,
        url(&desc_id),
        &alice.token,
        json!({ "score": 100, "metadata": "{\"tag\":\"first\"}" }),
        "write the top record",
    )
    .await;
    assert_eq!(first["leaderboard_id"], desc_id);
    assert_eq!(first["owner_id"], alice.uid);
    assert_eq!(first["username"], alice.username);
    assert_eq!(first["score"], "100");
    assert_eq!(first["num_score"], 1);
    assert_eq!(first["rank"], "1");
    assert_eq!(json_string(&first["metadata"]), json!({ "tag": "first" }));
    assert!(first.get("expiry_time").is_none());

    let lower = api_post(
        client,
        url(&desc_id),
        &alice.token,
        json!({ "score": 50 }),
        "write a lower best score",
    )
    .await;
    assert_eq!(lower["score"], "100");
    assert_eq!(lower["num_score"], 1);

    api_post(
        client,
        url(&desc_id),
        &bob.token,
        json!({ "score": 50 }),
        "write bob's record",
    )
    .await;
    api_post(
        client,
        url(&desc_id),
        &charlie.token,
        json!({ "score": 75 }),
        "write charlie's record",
    )
    .await;

    let page = api_get(
        client,
        with_query(&url(&desc_id), &[("limit", "2")]),
        &alice.token,
        "first leaderboard page",
    )
    .await;
    let scores: Vec<&str> = page["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(scores, vec!["100", "75"]);
    assert_eq!(page["records"][0]["rank"], "1");
    assert_eq!(page["records"][1]["rank"], "2");
    assert_eq!(page["rank_count"], "3");
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a leaderboard cursor")
        .to_owned();
    let page = api_get(
        client,
        with_query(&url(&desc_id), &[("limit", "2"), ("cursor", &cursor)]),
        &alice.token,
        "second leaderboard page",
    )
    .await;
    let records = page["records"].as_array().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["owner_id"], bob.uid);
    assert_eq!(records[0]["rank"], "3");
    assert!(page["prev_cursor"].is_string());
    assert!(page.get("next_cursor").is_none());

    let default_page = api_get(client, url(&desc_id), &alice.token, "default leaderboard page").await;
    assert_eq!(default_page["records"].as_array().expect("records").len(), 1);

    let owners = format!("{},{}", alice.uid, bob.uid);
    let owned = api_get(
        client,
        with_query(&url(&desc_id), &[("owner_ids", &owners)]),
        &alice.token,
        "owner records batch",
    )
    .await;
    assert!(owned.get("records").is_none());
    let owner_scores: Vec<&str> = owned["owner_records"]
        .as_array()
        .expect("owner records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(owner_scores, vec!["100", "50"]);

    for (pairs, what) in [
        (vec![("owner_ids", "not-a-uuid")], "list with a malformed owner"),
        (vec![("limit", "0")], "list with a zero limit"),
        (vec![("limit", "1001")], "list above the page ceiling"),
        (vec![("limit", "abc")], "list with a non-numeric limit"),
        (vec![("cursor", "garbage")], "list with a malformed cursor"),
        (vec![("expiry", "abc")], "list with a non-numeric expiry"),
    ] {
        refusal(
            client
                .get(with_query(&url(&desc_id), &pairs))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }
    let expired = api_get(
        client,
        with_query(&url(&desc_id), &[("expiry", "1")]),
        &alice.token,
        "list a closed window",
    )
    .await;
    assert_eq!(expired, json!({}));

    let haystack = api_get(
        client,
        with_query(&url(&desc_id), &[("limit", "3")]),
        &alice.token,
        "haystack without an owner",
    )
    .await;
    assert_eq!(haystack["records"].as_array().expect("records").len(), 3);
    let around = api_get(
        client,
        with_query(
            &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", alice.uid),
            &[("limit", "3")],
        ),
        &alice.token,
        "alice's haystack",
    )
    .await;
    let scores: Vec<&str> = around["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(scores, vec!["100", "75", "50"]);
    assert_eq!(around["rank_count"], "3");
    let trimmed = api_get(
        client,
        with_query(
            &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", bob.uid),
            &[("limit", "1")],
        ),
        &bob.token,
        "bob's trimmed haystack",
    )
    .await;
    let records = trimmed["records"].as_array().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["owner_id"], bob.uid);
    assert_eq!(records[0]["rank"], "3");
    assert!(trimmed["prev_cursor"].is_string());
    let cursor_page = api_get(
        client,
        with_query(
            &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", alice.uid),
            &[("limit", "2"), ("cursor", &cursor)],
        ),
        &alice.token,
        "a haystack handed a cursor",
    )
    .await;
    assert!(cursor_page.get("owner_records").is_none());
    assert_eq!(cursor_page["records"].as_array().expect("records").len(), 1);
    let no_record = api_get(
        client,
        format!(
            "{base}/v2/leaderboard/{desc_id}/owner/{}",
            Uuid::new_v4()
        ),
        &alice.token,
        "a haystack for a player with no record",
    )
    .await;
    assert_eq!(no_record, json!({}));
    for (pairs, what) in [
        (vec![("limit", "0")], "haystack with a zero limit"),
        (vec![("limit", "101")], "haystack above its ceiling"),
        (vec![("expiry", "abc")], "haystack with a non-numeric expiry"),
    ] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", alice.uid),
                    &pairs,
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }
    refusal(
        client
            .get(format!(
                "{base}/v2/leaderboard/{desc_id}/owner/not-a-uuid"
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "haystack for a malformed owner",
    )
    .await;

    let ops = |score: i64, operator: i64| {
        json!({ "score": score, "operator": operator })
    };
    let written = api_post(client, url(&ops_id), &alice.token, ops(10, 0), "best 10").await;
    assert_eq!(written["score"], "10");
    api_post(client, url(&ops_id), &alice.token, ops(30, 0), "best 30").await;
    let unchanged = api_post(client, url(&ops_id), &alice.token, ops(5, 0), "best 5").await;
    assert_eq!(unchanged["score"], "30");
    assert_eq!(unchanged["num_score"], 2);
    let set = api_post(client, url(&ops_id), &alice.token, ops(40, 2), "set 40").await;
    assert_eq!(set["score"], "40");
    assert_eq!(set["num_score"], 3);
    let increment = api_post(client, url(&ops_id), &alice.token, ops(45, 3), "increment 45").await;
    assert_eq!(increment["score"], "85");
    let decrement = api_post(client, url(&ops_id), &alice.token, ops(100, 4), "decrement 100").await;
    assert!(decrement.get("score").is_none(), "a decrement floors at zero");
    refusal(
        client
            .post(url(&ops_id))
            .bearer_auth(&alice.token)
            .json(&ops(1, 9))
            .send()
            .await
            .unwrap(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "write with an unknown operator",
    )
    .await;
    refusal(
        client
            .post(url(&ops_id))
            .bearer_auth(&alice.token)
            .json(&json!({ "score": 1, "metadata": "[1]" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write a non-object metadata value",
    )
    .await;
    let empty_record = api_post(client, url(&ops_id), &alice.token, json!({}), "empty record").await;
    assert_eq!(empty_record["owner_id"], alice.uid);
    let removed = api_delete(
        client,
        url(&ops_id),
        &alice.token,
        "delete an own record",
    )
    .await;
    assert_eq!(removed, json!({}));
    let after_delete = api_get(
        client,
        with_query(&url(&ops_id), &[("owner_ids", &alice.uid)]),
        &alice.token,
        "list after deleting the only record",
    )
    .await;
    assert_eq!(after_delete, json!({}));

    for (player, score) in [(&alice, 10), (&bob, 5), (&charlie, 7)] {
        api_post(
            client,
            url(&asc_id),
            &player.token,
            json!({ "score": score }),
            "write an ascending record",
        )
        .await;
    }
    let ascending = api_get(
        client,
        with_query(&url(&asc_id), &[("limit", "10")]),
        &alice.token,
        "ascending listing",
    )
    .await;
    let scores: Vec<&str> = ascending["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(scores, vec!["5", "7", "10"]);

    refusal(
        client
            .post(url(&auth_id))
            .bearer_auth(&alice.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "write to an authoritative leaderboard",
    )
    .await;
    refusal(
        client
            .delete(url(&auth_id))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "delete from an authoritative leaderboard",
    )
    .await;

    let unranked = api_post(
        client,
        url(&unranked_id),
        &alice.token,
        json!({ "score": 12 }),
        "write to an unranked board",
    )
    .await;
    assert!(unranked.get("rank").is_none());
    let unranked_list = api_get(
        client,
        with_query(&url(&unranked_id), &[("limit", "10")]),
        &alice.token,
        "list an unranked board",
    )
    .await;
    assert!(unranked_list.get("rank_count").is_none());
    assert!(unranked_list["records"][0].get("rank").is_none());

    refusal(
        client
            .delete(url(&tournament_id))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete a tournament record through the leaderboard route",
    )
    .await;

    for response in [
        client
            .post(url("e2e-lb-absent"))
            .bearer_auth(&alice.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        client
            .get(url("e2e-lb-absent"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        client
            .delete(url("e2e-lb-absent"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
    ] {
        refusal(response, StatusCode::NOT_FOUND, "address an unknown leaderboard").await;
    }

    exec(
        pool,
        "DELETE type::record('leaderboard', $id) RETURN NOTHING",
        &[("id", json!(tournament_id))],
    )
    .await;
}
