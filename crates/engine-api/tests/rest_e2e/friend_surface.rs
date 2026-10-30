use super::*;

pub async fn friend_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: friends");
    let alice = device_player(client, base, "fr-a").await;
    let bob = device_player(client, base, "fr-b").await;
    let carol = device_player(client, base, "fr-c").await;
    let (a, b, c) = (uuid_of(&alice), uuid_of(&bob), uuid_of(&carol));

    let empty = api_get(
        client,
        format!("{base}/v2/friend"),
        &alice.token,
        "an empty friend list",
    )
    .await;
    assert_eq!(empty["friends"], json!([]));
    assert_eq!(empty["cursor"], "");

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend?limit=0"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friend list limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 1000."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend?state=4"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friend list state out of range",
        )
        .await,
        "Invalid state - state must be between 0 and 3."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend?cursor=not-a-cursor"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friend list bad cursor",
        )
        .await,
        "Cursor is invalid."
    );

    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend"),
                    &[("ids", alice.uid.as_str())],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding self as a friend",
        )
        .await,
        "Cannot add self as friend."
    );
    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend"),
                    &[("usernames", "no-such-player-at-all")],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding an unresolvable username",
        )
        .await,
        "No valid ID or username was provided."
    );
    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend"),
                    &[("ids", "not-a-uuid")],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding a malformed id",
        )
        .await,
        "Invalid user ID 'not-a-uuid'."
    );

    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[("ids", bob.uid.as_str()), ("metadata", r#"{"nick":"bob"}"#)],
            ),
            &alice.token,
            "alice invites bob",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, a, b).await, Some(1));
    assert_eq!(friend_edge_state(pool, b, a).await, Some(2));
    assert_eq!(edge_count(pool, a).await, 1);
    assert_eq!(edge_count(pool, b).await, 1);

    let inbox = api_get(
        client,
        format!("{base}/v2/notification?limit=10&sender_id={}", alice.uid),
        &bob.token,
        "bob's friend-request notification",
    )
    .await;
    let rows = inbox["notifications"]
        .as_array()
        .expect("bob has a notification");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["code"], -2);
    assert_eq!(rows[0]["sender_id"], alice.uid);
    assert_eq!(
        rows[0]["subject"],
        format!("{} wants to add you as a friend", alice.username)
    );
    assert_eq!(json_string(&rows[0]["content"])["username"], alice.username);
    assert_eq!(rows[0]["persistent"], true);

    let pending = api_get(
        client,
        format!("{base}/v2/friend?state=1"),
        &alice.token,
        "alice's sent invites",
    )
    .await;
    let rows = pending["friends"].as_array().expect("a sent invite");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["user"]["id"], bob.uid);
    assert_eq!(rows[0]["state"], 1);
    assert_eq!(json_string(&rows[0]["metadata"]), json!({ "nick": "bob" }));
    assert!(rows[0]["update_time"].as_str().is_some());

    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[
                    ("ids", alice.uid.as_str()),
                    ("metadata", r#"{"since":"accepted"}"#),
                ],
            ),
            &bob.token,
            "bob accepts alice's invite",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, a, b).await, Some(0));
    assert_eq!(friend_edge_state(pool, b, a).await, Some(0));
    assert_eq!(edge_count(pool, a).await, 1);
    assert_eq!(edge_count(pool, b).await, 1);
    let bob_meta = row(
        pool,
        "SELECT metadata FROM user_edge
         WHERE (source_id = <uuid>$from OR source_id = $from)
           AND (destination_id = <uuid>$to OR destination_id = $to) LIMIT 1",
        &[("from", json!(b.to_string())), ("to", json!(a.to_string()))],
    )
    .await
    .expect("bob's edge metadata")["metadata"]
        .clone();
    assert_eq!(bob_meta, json!({ "since": "accepted" }));

    let accepted = api_get(
        client,
        format!("{base}/v2/notification?limit=10&sender_id={}", bob.uid),
        &alice.token,
        "alice's acceptance notification",
    )
    .await;
    assert!(
        accepted["notifications"]
            .as_array()
            .expect("alice has notifications")
            .iter()
            .any(|n| n["code"] == -3
                && n["subject"] == format!("{} accepted your friend request", bob.username))
    );

    api_post_empty(
        client,
        with_query(&format!("{base}/v2/friend"), &[("ids", carol.uid.as_str())]),
        &bob.token,
        "bob invites carol",
    )
    .await;
    api_post_empty(
        client,
        with_query(&format!("{base}/v2/friend"), &[("ids", bob.uid.as_str())]),
        &carol.token,
        "carol accepts bob's invite",
    )
    .await;
    assert_eq!(friend_edge_state(pool, b, c).await, Some(0));

    let fof = api_get(
        client,
        format!("{base}/v2/friend/friends?limit=10"),
        &alice.token,
        "alice's friends of friends",
    )
    .await;
    let entries = fof["friends_of_friends"]
        .as_array()
        .expect("a friends-of-friends page");
    let entry = entries
        .iter()
        .find(|entry| entry["user"]["id"] == carol.uid)
        .expect("carol is reachable through bob");
    assert_eq!(entry["referrer"], bob.uid);
    assert!(
        entries
            .iter()
            .all(|entry| entry["user"]["id"] != alice.uid && entry["user"]["id"] != bob.uid)
    );
    assert_eq!(fof["cursor"], "");

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend/friends?limit=0"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friends-of-friends limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );

    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend/block"),
                    &[("ids", carol.uid.as_str())],
                ))
                .bearer_auth(&carol.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "blocking self",
        )
        .await,
        "Cannot block self."
    );
    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend/block"),
                &[("ids", alice.uid.as_str())],
            ),
            &carol.token,
            "carol blocks alice",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, c, a).await, Some(3));
    assert_eq!(edge_count(pool, c).await, 2);

    assert_eq!(
        api_post_empty(
            client,
            with_query(&format!("{base}/v2/friend"), &[("ids", carol.uid.as_str())],),
            &alice.token,
            "alice invites a player who blocked her",
        )
        .await,
        json!({})
    );
    assert_eq!(
        friend_edge_state(pool, a, c).await,
        None,
        "a blocked invite writes nothing"
    );

    let blocked = api_get(
        client,
        format!("{base}/v2/friend?state=3"),
        &carol.token,
        "carol's blocked list",
    )
    .await;
    assert_eq!(blocked["friends"][0]["user"]["id"], alice.uid);
    assert_eq!(blocked["friends"][0]["state"], 3);

    assert_eq!(
        api_delete(
            client,
            with_query(&format!("{base}/v2/friend"), &[("ids", carol.uid.as_str())],),
            &bob.token,
            "bob deletes carol",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, b, c).await, None);
    assert_eq!(friend_edge_state(pool, c, b).await, None);
    assert_eq!(edge_count(pool, b).await, 1);
    assert_eq!(edge_count(pool, c).await, 1);
    let removed = api_get(
        client,
        format!("{base}/v2/notification?limit=10&sender_id={}", bob.uid),
        &carol.token,
        "carol's removal notification",
    )
    .await;
    assert!(
        removed["notifications"]
            .as_array()
            .expect("carol has notifications")
            .iter()
            .any(|n| n["code"] == -9
                && n["subject"] == format!("{} removed you as a friend", bob.username))
    );

    assert_eq!(
        api_delete(
            client,
            with_query(&format!("{base}/v2/friend"), &[("ids", bob.uid.as_str())],),
            &alice.token,
            "alice deletes bob",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, a, b).await, None);
    assert_eq!(friend_edge_state(pool, b, a).await, None);
    assert_eq!(edge_count(pool, a).await, 0);
    assert_eq!(edge_count(pool, b).await, 0);

    assert_eq!(
        refusal(
            client
                .delete(with_query(
                    &format!("{base}/v2/friend"),
                    &[("ids", alice.uid.as_str())],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "deleting self as a friend",
        )
        .await,
        "Cannot delete self."
    );
    assert_eq!(
        api_delete(
            client,
            format!("{base}/v2/friend"),
            &alice.token,
            "deleting no friends",
        )
        .await,
        json!({})
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/friend/facebook"))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "facebook import",
        )
        .await,
        "Facebook authentication is not configured."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/friend/steam"))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "steam import",
        )
        .await,
        "Steam authentication is not configured."
    );
}
