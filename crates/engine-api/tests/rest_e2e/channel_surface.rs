use super::*;

pub async fn channel_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: channels");
    let alice = device_player(client, base, "ch-a").await;
    let bob = device_player(client, base, "ch-b").await;
    let charlie = device_player(client, base, "ch-c").await;

    let room_label = format!("e2e-room-{}", letters_suffix());
    let room = format!("2...{room_label}");

    for bad in [
        "not-a-channel".to_owned(),
        "1.aa.bb.cc".to_owned(),
        "3....".to_owned(),
        format!("2...{}", "r".repeat(65)),
    ] {
        refusal(
            client
                .get(format!("{base}/v2/channel/{bad}"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list a malformed channel",
        )
        .await;
    }

    let empty = api_get(
        client,
        with_query(&format!("{base}/v2/channel/{room}"), &[("limit", "10")]),
        &alice.token,
        "empty room listing",
    )
    .await;
    assert_eq!(empty["messages"], json!([]));
    assert!(empty.get("next_cursor").is_none());
    assert!(empty.get("cacheable_cursor").is_none());

    for bad in [json!({ "content": "plain" }), json!({ "content": 7 })] {
        refusal(
            client
                .post(format!("{base}/v2/channel/{room}/message"))
                .bearer_auth(&alice.token)
                .json(&bad)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "send a non-object message",
        )
        .await;
    }

    let first = api_post(
        client,
        format!("{base}/v2/channel/{room}/message"),
        &alice.token,
        json!({ "content": { "text": "hello" } }),
        "send the first room message",
    )
    .await;
    assert_eq!(first["channel_id"], room);
    assert_eq!(first["sender_id"], alice.uid);
    assert_eq!(first["username"], alice.username);
    assert_eq!(first["code"], 0);
    assert_eq!(first["persistent"], true);
    assert_eq!(first["room_name"], room_label);
    assert_eq!(json_string(&first["content"]), json!({ "text": "hello" }));
    let first_id = first["message_id"]
        .as_str()
        .expect("first message id")
        .to_owned();

    let second = api_post(
        client,
        format!("{base}/v2/channel/{room}/message"),
        &bob.token,
        json!({ "content": [{ "text": "second" }] }),
        "send the second room message",
    )
    .await;
    let second_id = second["message_id"]
        .as_str()
        .expect("second message id")
        .to_owned();
    assert_eq!(second["sender_id"], bob.uid);

    let third = api_post(
        client,
        format!("{base}/v2/channel/{room}/message"),
        &alice.token,
        json!({ "content": { "text": "third" } }),
        "send the third room message",
    )
    .await;
    let third_id = third["message_id"]
        .as_str()
        .expect("third message id")
        .to_owned();

    let page = api_get(
        client,
        with_query(&format!("{base}/v2/channel/{room}"), &[("limit", "2")]),
        &alice.token,
        "first forward page",
    )
    .await;
    let messages = page["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["message_id"], first_id);
    assert_eq!(messages[1]["message_id"], second_id);
    let next = page["next_cursor"]
        .as_str()
        .expect("forward page has a next cursor")
        .to_owned();
    assert!(page.get("prev_cursor").is_none());

    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/channel/{room}"),
            &[("limit", "2"), ("cursor", &next)],
        ),
        &alice.token,
        "second forward page",
    )
    .await;
    let messages = page["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["message_id"], third_id);
    assert!(page.get("next_cursor").is_none());
    assert!(page["prev_cursor"].is_string());

    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/channel/{room}"),
            &[("limit", "2"), ("forward", "false")],
        ),
        &alice.token,
        "backward page",
    )
    .await;
    let messages = page["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["message_id"], third_id);
    assert_eq!(messages[1]["message_id"], second_id);

    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/channel/{room}"),
                &[("limit", "2"), ("cursor", &next), ("forward", "false")],
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "replay a forward cursor backward",
    )
    .await;
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/channel/{room}"),
                &[("cursor", "garbage")],
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "list a channel with a malformed cursor",
    )
    .await;
    for limit in ["0", "101"] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/channel/{room}"),
                    &[("limit", limit)],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list a channel with a bad limit",
        )
        .await;
    }

    refusal(
        client
            .put(format!("{base}/v2/channel/{room}/message/{first_id}"))
            .bearer_auth(&bob.token)
            .json(&json!({ "content": { "text": "hijack" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "edit another sender's message",
    )
    .await;
    let edited = api_put(
        client,
        format!("{base}/v2/channel/{room}/message/{first_id}"),
        &alice.token,
        json!({ "content": { "text": "edited" } }),
        "edit an own message",
    )
    .await;
    assert_eq!(json_string(&edited["content"]), json!({ "text": "edited" }));
    assert_eq!(edited["message_id"], first_id);

    refusal(
        client
            .put(format!("{base}/v2/channel/{room}/message/not-a-uuid"))
            .bearer_auth(&alice.token)
            .json(&json!({ "content": {} }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "edit a malformed message id",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/channel/{room}/message/not-a-uuid"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a malformed message id",
    )
    .await;

    let tombstone = api_delete(
        client,
        format!("{base}/v2/channel/{room}/message/{first_id}"),
        &alice.token,
        "delete an own message",
    )
    .await;
    assert_eq!(tombstone["message_id"], first_id);
    assert_eq!(tombstone["code"], 2);

    let (subject, descriptor) = if alice.uid < bob.uid {
        (alice.uid.clone(), bob.uid.clone())
    } else {
        (bob.uid.clone(), alice.uid.clone())
    };
    let dm = format!("4.{subject}.{descriptor}.");
    let dm_message = api_post(
        client,
        format!("{base}/v2/channel/{dm}/message"),
        &alice.token,
        json!({ "content": { "text": "psst" } }),
        "send a DM",
    )
    .await;
    assert_eq!(dm_message["user_id_one"], subject);
    assert_eq!(dm_message["user_id_two"], descriptor);
    assert!(dm_message.get("group_id").is_none());
    assert!(dm_message.get("room_name").is_none());

    let dm_page = api_get(
        client,
        format!("{base}/v2/channel/{dm}"),
        &bob.token,
        "read a DM as its other participant",
    )
    .await;
    assert_eq!(dm_page["messages"].as_array().expect("messages").len(), 1);
    refusal(
        client
            .get(format!("{base}/v2/channel/{dm}"))
            .bearer_auth(&charlie.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read somebody else's DM",
    )
    .await;
    let reversed = format!("4.{descriptor}.{subject}.");
    refusal(
        client
            .get(format!("{base}/v2/channel/{reversed}"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "use a non-canonical DM id",
    )
    .await;
    let self_dm = format!("4.{}.{}.", alice.uid, alice.uid);
    refusal(
        client
            .get(format!("{base}/v2/channel/{self_dm}"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "address a DM to yourself",
    )
    .await;

    let group_name = format!("e2e-channel-group-{}", letters_suffix());
    let group = api_post(
        client,
        format!("{base}/v2/group"),
        &alice.token,
        json!({ "name": group_name, "open": true }),
        "create a group for its channel",
    )
    .await;
    let gid = group["group"]["id"].as_str().expect("group id").to_owned();
    let gchannel = format!("3.{gid}..");
    api_post(
        client,
        format!("{base}/v2/channel/{gchannel}/message"),
        &alice.token,
        json!({ "content": { "text": "members only" } }),
        "send to a group channel",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/channel/{gchannel}"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read a group channel as a non-member",
    )
    .await;
    api_post_empty(
        client,
        format!("{base}/v2/group/{gid}/join"),
        &bob.token,
        "join the group to read its channel",
    )
    .await;
    let member_page = api_get(
        client,
        format!("{base}/v2/channel/{gchannel}"),
        &bob.token,
        "read a group channel as a member",
    )
    .await;
    assert_eq!(
        member_page["messages"].as_array().expect("messages").len(),
        1
    );
    assert_eq!(member_page["messages"][0]["group_id"], gid);
    assert!(member_page["messages"][0].get("user_id_one").is_none());
    api_post_empty(
        client,
        format!("{base}/v2/group/{gid}/leave"),
        &bob.token,
        "leave the group channel",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/channel/{gchannel}"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read a group channel after leaving",
    )
    .await;

    let stored = count(
        pool,
        "message",
        Some("(stream_label = $label OR (stream_subject = <uuid>$gid OR stream_subject = $gid))"),
        &[
            ("label", json!(room_label.as_str())),
            ("gid", json!(gid.as_str())),
        ],
    )
    .await;
    assert_eq!(stored, 4, "3 room rows plus 1 group row");
}
