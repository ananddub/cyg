use super::*;

pub async fn notification_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: notifications");
    let alice = device_player(client, base, "no-a").await;
    let bob = device_player(client, base, "no-b").await;
    let charlie = device_player(client, base, "no-c").await;

    let add = |username: &str| {
        with_query(
            &format!("{base}/v2/friend"),
            &[("usernames", username)],
        )
    };

    api_post_empty(
        client,
        add(&bob.username),
        &alice.token,
        "alice invites bob",
    )
    .await;
    api_post_empty(
        client,
        add(&alice.username),
        &bob.token,
        "bob accepts alice",
    )
    .await;

    let group_name = format!("e2e-notify-group-{}", letters_suffix());
    let group = api_post(
        client,
        format!("{base}/v2/group"),
        &alice.token,
        json!({ "name": group_name, "open": false }),
        "create a closed group for notifications",
    )
    .await;
    let gid = group["group"]["id"].as_str().expect("group id").to_owned();
    api_post_empty(
        client,
        format!("{base}/v2/group/{gid}/join"),
        &bob.token,
        "bob requests to join",
    )
    .await;
    api_post_empty(
        client,
        with_query(
            &format!("{base}/v2/group/{gid}/add"),
            &[("user_ids", &bob.uid)],
        ),
        &alice.token,
        "alice adds bob",
    )
    .await;
    api_delete(
        client,
        add(&bob.username),
        &alice.token,
        "alice removes bob",
    )
    .await;

    let bob_page = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "bob's notifications",
    )
    .await;
    let rows = bob_page["notifications"]
        .as_array()
        .expect("bob has notifications")
        .clone();
    assert_eq!(rows.len(), 3, "{bob_page}");
    let mut codes: Vec<i64> = rows
        .iter()
        .map(|row| row["code"].as_i64().expect("a code"))
        .collect();
    codes.sort_unstable();
    assert_eq!(codes, vec![-9, -4, -2]);
    for row in &rows {
        assert_eq!(row["sender_id"], alice.uid);
        assert_eq!(row["persistent"], true);
        assert!(row["create_time"].as_str().is_some_and(|t| t.ends_with('Z')));
    }
    let by_code = |page: &Value, code: i16| -> Value {
        page["notifications"]
            .as_array()
            .expect("notifications")
            .iter()
            .find(|row| row["code"] == code)
            .unwrap_or_else(|| panic!("no notification with code {code}: {page}"))
            .clone()
    };
    let invite = by_code(&bob_page, -2);
    assert_eq!(json_string(&invite["content"]), json!({ "username": alice.username }));
    assert_eq!(
        invite["subject"],
        format!("{} wants to add you as a friend", alice.username)
    );
    let added = by_code(&bob_page, -4);
    assert_eq!(
        json_string(&added["content"]),
        json!({ "group_id": gid, "name": group_name })
    );
    assert_eq!(added["subject"], format!("You've been added to group {group_name}"));
    let removed = by_code(&bob_page, -9);
    assert_eq!(json_string(&removed["content"]), json!({ "username": alice.username }));
    assert_eq!(
        removed["subject"],
        format!("{} removed you as a friend", alice.username)
    );

    let alice_page = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &alice.token,
        "alice's notifications",
    )
    .await;
    let alice_rows = alice_page["notifications"]
        .as_array()
        .expect("alice has notifications");
    assert_eq!(alice_rows.len(), 2);
    let mut alice_codes: Vec<i64> = alice_rows
        .iter()
        .map(|row| row["code"].as_i64().expect("a code"))
        .collect();
    alice_codes.sort_unstable();
    assert_eq!(alice_codes, vec![-5, -3]);
    let requested = by_code(&alice_page, -5);
    assert_eq!(json_string(&requested["content"])["group_id"], gid);
    assert_eq!(json_string(&requested["content"])["username"], bob.username);
    assert_eq!(
        requested["subject"],
        format!("User {} wants to join your group", bob.username)
    );
    let accepted = by_code(&alice_page, -3);
    assert_eq!(json_string(&accepted["content"]), json!({ "username": bob.username }));

    let filtered = api_get(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("limit", "100"), ("code", "-2")],
        ),
        &bob.token,
        "filter notifications by code",
    )
    .await;
    assert_eq!(filtered["notifications"].as_array().expect("notifications").len(), 1);
    assert_eq!(filtered["notifications"][0]["code"], -2);
    let by_sender = api_get(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("limit", "100"), ("sender_id", &alice.uid)],
        ),
        &bob.token,
        "filter notifications by sender",
    )
    .await;
    assert_eq!(by_sender["notifications"].as_array().expect("notifications").len(), 3);
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/notification"),
                &[("sender_id", "not-a-uuid")],
            ))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "filter notifications by a malformed sender",
    )
    .await;
    for limit in ["0", "101"] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/notification"),
                    &[("limit", limit)],
                ))
                .bearer_auth(&bob.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list notifications with a bad limit",
        )
        .await;
    }
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/notification"),
                &[("cacheableCursor", "garbage")],
            ))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "list notifications with a malformed cursor",
    )
    .await;

    let default_page = api_get(
        client,
        format!("{base}/v2/notification"),
        &alice.token,
        "default notification page",
    )
    .await;
    assert_eq!(default_page["notifications"].as_array().expect("notifications").len(), 1);

    let mut seen: Vec<String> = Vec::new();
    let mut cursor = String::new();
    for _ in 0..3 {
        let mut pairs: Vec<(&str, &str)> = vec![("limit", "1")];
        if !cursor.is_empty() {
            pairs.push(("cacheableCursor", &cursor));
        }
        let page = api_get(
            client,
            with_query(&format!("{base}/v2/notification"), &pairs),
            &bob.token,
            "page through notifications",
        )
        .await;
        let rows = page["notifications"].as_array().expect("notifications");
        assert_eq!(rows.len(), 1);
        seen.push(rows[0]["id"].as_str().expect("notification id").to_owned());
        cursor = page["cacheable_cursor"]
            .as_str()
            .expect("a cacheable cursor")
            .to_owned();
    }
    assert_eq!(seen.len(), 3);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 3);

    let charlie_page = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &charlie.token,
        "a player with no notifications",
    )
    .await;
    assert!(charlie_page.get("notifications").is_none());
    assert!(!charlie_page["cacheable_cursor"].as_str().unwrap_or("").is_empty());

    let single = removed["id"].as_str().expect("notification id").to_owned();
    api_delete(
        client,
        format!("{base}/v2/notification/{single}"),
        &bob.token,
        "delete one notification",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/notification/not-a-uuid"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a malformed notification id",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/notification/{}", Uuid::new_v4()))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete an unknown notification",
    )
    .await;

    let remaining: Vec<String> = bob_page["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .filter(|id| id != &single)
        .collect();
    assert_eq!(remaining.len(), 2);
    api_delete(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("ids", &remaining[0]), ("ids", &remaining[1])],
        ),
        &bob.token,
        "delete notifications by repeated ids",
    )
    .await;
    let cleared = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "bob has no notifications left",
    )
    .await;
    assert!(cleared.get("notifications").is_none());
    assert!(!cleared["cacheable_cursor"].as_str().unwrap_or("").is_empty());

    api_post_empty(client, add(&bob.username), &alice.token, "invite bob again").await;
    let invite = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "the second invite",
    )
    .await;
    let invite_id = invite["notifications"][0]["id"]
        .as_str()
        .expect("a notification id")
        .to_owned();
    api_delete_body(
        client,
        format!("{base}/v2/notification"),
        &bob.token,
        json!({ "ids": [invite_id] }),
        "delete notifications by JSON body",
    )
    .await;

    api_delete(client, add(&bob.username), &alice.token, "remove bob again").await;
    let removed = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "the second removal",
    )
    .await;
    let removed_id = removed["notifications"][0]["id"]
        .as_str()
        .expect("a notification id")
        .to_owned();
    let array_value = json!([removed_id]).to_string();
    api_delete(
        client,
        with_query(&format!("{base}/v2/notification"), &[("ids", &array_value)]),
        &bob.token,
        "delete notifications by a JSON-array query value",
    )
    .await;

    api_post_empty(client, add(&bob.username), &alice.token, "invite bob once more").await;
    api_delete(client, add(&bob.username), &alice.token, "remove bob once more").await;
    let pairs = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "the final pair of notifications",
    )
    .await;
    let ids: Vec<String> = pairs["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect();
    assert_eq!(ids.len(), 2);
    api_delete(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("ids", &ids.join(","))],
        ),
        &bob.token,
        "delete notifications by a comma-joined list",
    )
    .await;

    api_delete_body(
        client,
        format!("{base}/v2/notification"),
        &bob.token,
        json!({ "ids": [] }),
        "delete an empty notification list",
    )
    .await;

    let left = count(
        pool,
        "notification",
        Some("(user_id = <uuid>$u OR user_id = $u)"),
        &[("u", json!(bob.uid.as_str()))],
    )
    .await;
    assert_eq!(left, 0);
}
