use super::*;

pub async fn channel_surface(env: &Harness) {
    eprintln!("gap: channel messages");
    let author = device_player(&env.client, &env.base, "chat").await;
    let mate = device_player(&env.client, &env.base, "chum").await;
    let outsider = device_player(&env.client, &env.base, "outr").await;

    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/2", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a channel id that is not four dotted fields",
        )
        .await,
        "Invalid channel ID."
    );

    let room = format!("2...e2e-gap-room-{}", letters_suffix());
    let empty = json_body(
        env.client
            .get(format!("{}/v2/channel/{room}", env.base))
            .bearer_auth(&author.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId} (empty room)",
    )
    .await;
    assert_eq!(empty["messages"].as_array().map(Vec::len), Some(0), "{empty}");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/channel/{room}/message", env.base))
                .bearer_auth(&author.token)
                .json(&json!({ "content": "hello" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "content that is neither object nor array",
        )
        .await,
        "Message content must be a JSON object."
    );
    let sent = json_body(
        env.client
            .post(format!("{}/v2/channel/{room}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "gap first line" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/channel/{channelId}/message",
    )
    .await;
    assert_eq!(sent["channel_id"], room);
    assert_eq!(sent["sender_id"], author.uid);
    assert_eq!(sent["username"], author.username);
    assert_eq!(json_string(&sent["content"]), json!({ "text": "gap first line" }));
    assert_eq!(sent["room_name"], room.strip_prefix("2...").unwrap());
    let message_id = sent["message_id"].as_str().expect("message id").to_owned();
    json_body(
        env.client
            .post(format!("{}/v2/channel/{room}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "gap second line" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a second room message",
    )
    .await;

    let head = json_body(
        env.client
            .get(format!("{}/v2/channel/{room}?limit=10", env.base))
            .bearer_auth(&author.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId}?limit=10",
    )
    .await;
    let rows = head["messages"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 2, "{head}");
    assert_eq!(
        json_string(&rows[0]["content"]),
        json!({ "text": "gap first line" }),
        "forward reads oldest first"
    );
    assert!(head["cacheable_cursor"].is_string());
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{room}?limit=0", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the log refusing limit=0",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{room}?cursor=not-a-cursor", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "the log refusing a malformed cursor",
        )
        .await,
        "Cursor is invalid or expired."
    );

    let edited = json_body(
        env.client
            .put(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "gap edited" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/channel/{channelId}/message/{messageId}",
    )
    .await;
    assert_eq!(json_string(&edited["content"]), json!({ "text": "gap edited" }));
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
                .bearer_auth(&mate.token)
                .json(&json!({ "content": { "text": "not yours" } }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "editing somebody else's message",
        )
        .await,
        "Channel message not found."
    );
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/channel/{room}/message/not-a-uuid", env.base))
                .bearer_auth(&author.token)
                .json(&json!({ "content": { "text": "?" } }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an edit aimed at a malformed message id",
        )
        .await,
        "Invalid message ID."
    );

    let tomb = json_body(
        env.client
            .delete(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
            .bearer_auth(&author.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/channel/{channelId}/message/{messageId}",
    )
    .await;
    assert_eq!(tomb["code"], 2, "the removal is a tombstone, not a vanishing");
    assert_eq!(tomb["content"], "{}");
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/channel/{room}/message/{message_id}", env.base))
                .bearer_auth(&mate.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "removing somebody else's message",
        )
        .await,
        "Channel message not found."
    );

    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&author.token)
            .json(&json!({
                "name": format!("e2e talkie {}", letters_suffix()),
                "open": true,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the talking group",
    )
    .await;
    let gid = created["group"]["id"].as_str().expect("group id").to_owned();
    let group_channel = format!("3.{gid}..");
    json_body(
        env.client
            .post(format!("{}/v2/channel/{group_channel}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "group hello" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/channel/{channelId}/message (group stream)",
    )
    .await;
    expect(
        env.client
            .post(format!("{}/v2/group/{gid}/join", env.base))
            .bearer_auth(&mate.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the mate joining the group",
    )
    .await;
    let log = json_body(
        env.client
            .get(format!("{}/v2/channel/{group_channel}?limit=5", env.base))
            .bearer_auth(&mate.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId} (group stream, member)",
    )
    .await;
    assert_eq!(log["messages"].as_array().map(Vec::len), Some(1), "{log}");
    assert_eq!(log["messages"][0]["group_id"], gid);
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{group_channel}", env.base))
                .bearer_auth(&outsider.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a non-member reading the group stream",
        )
        .await,
        "Group not found."
    );

    let (small, large) = if author.uid < mate.uid {
        (author.uid.clone(), mate.uid.clone())
    } else {
        (mate.uid.clone(), author.uid.clone())
    };
    let dm = format!("4.{small}.{large}.");
    json_body(
        env.client
            .post(format!("{}/v2/channel/{dm}/message", env.base))
            .bearer_auth(&author.token)
            .json(&json!({ "content": { "text": "dm ping" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a DM from one half of the pair",
    )
    .await;
    let box_ = json_body(
        env.client
            .get(format!("{}/v2/channel/{dm}?limit=5", env.base))
            .bearer_auth(&mate.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/channel/{channelId} (DM stream)",
    )
    .await;
    assert_eq!(box_["messages"].as_array().map(Vec::len), Some(1), "{box_}");
    assert_eq!(box_["messages"][0]["user_id_one"], small);
    assert_eq!(box_["messages"][0]["user_id_two"], large);
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/{dm}", env.base))
                .bearer_auth(&outsider.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a stranger reading somebody's DM",
        )
        .await,
        "Channel not found."
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/channel/4.{large}.{small}.", env.base))
                .bearer_auth(&author.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the DM ids out of canonical order",
        )
        .await,
        "Invalid channel ID."
    );
}
