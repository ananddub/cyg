use super::*;

pub async fn chat_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: chat plane (room channel, ephemeral)");

    let cid = a
        .send_event("channel_join", json!({"channel_id": "bogus"}))
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "joining a malformed channel").await,
        "Invalid channel ID."
    );
    let cid = a
        .send_event("channel_join", json!({"channel_id": "2...lobby"}))
        .await;
    let joined_a = a.take_reply(&cid, Some("channel"), "channel join a").await;
    assert_eq!(joined_a["channel"]["id"], "2...lobby");
    assert_eq!(joined_a["channel"]["room_name"], "lobby");

    let cid = b
        .send_event("channel_join", json!({"channel_id": "2...lobby"}))
        .await;
    let joined_b = b.take_reply(&cid, Some("channel"), "channel join b").await;
    assert_eq!(
        joined_b["presences"].as_array().map(Vec::len),
        Some(1),
        "the roster already holds a: {joined_b}"
    );
    let arrival = a.take_event("channel_presence_event").await;
    assert_eq!(arrival["channel_id"], "2...lobby");
    assert_eq!(arrival["joins"].as_array().map(Vec::len), Some(1));

    let cid = b
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby", "content": {"body": "ping"}, "ephemeral": true}),
        )
        .await;
    let ack = b
        .take_reply(&cid, Some("channel_message_ack"), "ephemeral message")
        .await;
    assert_eq!(ack["code"], 0);
    let message = a.take_event("channel_message").await;
    assert_eq!(message["code"], 0);
    assert!(
        message["content"]
            .as_str()
            .unwrap_or_default()
            .contains("ping"),
        "content travels stringified: {message}"
    );

    a.fire(
        "stream_data",
        json!({"stream_id": "2...lobby", "data": {"input": "w"}, "reliable": false}),
    )
    .await;
    let stream = b.take_event("stream_data").await;
    assert_eq!(stream["stream_id"], "2...lobby");
    assert_eq!(stream["data"], json!({"input": "w"}));
    let cid = b
        .send_event(
            "stream_data",
            json!({"stream_id": "2...lobby", "data": {"input": "j"}, "reliable": true}),
        )
        .await;
    let echo = b
        .take_reply(&cid, Some("stream_data"), "reliable stream echo")
        .await;
    assert_eq!(echo["reliable"], true);
    a.take_event("stream_data").await;

    let cid = a
        .send_event("channel_leave", json!({"channel_id": "2...lobby"}))
        .await;
    let left = a
        .take_reply(&cid, Some("channel_leave"), "channel leave")
        .await;
    assert_eq!(left["channel_id"], "2...lobby");
    let departure = b.take_event("channel_presence_event").await;
    assert_eq!(departure["leaves"].as_array().map(Vec::len), Some(1));
    let cid = a
        .send_event("channel_leave", json!({"channel_id": "2...ghost"}))
        .await;
    a.take_reply(&cid, Some("channel_leave"), "leaving an unknown channel")
        .await;

    let cid = a
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby", "content": "x", "ephemeral": true}),
        )
        .await;
    let ack = a
        .take_reply(&cid, Some("channel_message_ack"), "send after leaving")
        .await;
    assert_eq!(ack["code"], 0);
    let overheard = b.take_event("channel_message").await;
    assert_eq!(overheard["username"], a.user.as_str());
    a.expect_quiet_for(
        Duration::from_millis(200),
        "the sender never hears its own message",
    )
    .await;

    let cid = b
        .send_event("channel_leave", json!({"channel_id": "2...lobby"}))
        .await;
    b.take_reply(&cid, Some("channel_leave"), "last member leaves")
        .await;
    let cid = a
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby", "content": "x", "ephemeral": true}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "message on a room nobody holds")
            .await,
        "must join the channel before sending messages"
    );
    let cid = a
        .send_event(
            "stream_data",
            json!({"stream_id": "2...ghost", "data": {"input": "x"}}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "stream data without joining").await,
        "must join the stream before sending data"
    );
}
