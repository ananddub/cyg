use super::*;

pub async fn channel_messages_plane(env: &Harness, a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: persistent channel messages (database-backed)");

    let cid = a
        .send_event(
            "channel_message_update",
            json!({"channel_id": "2...lobby-db", "message_id": Uuid::new_v4().to_string(), "content": "x"}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "update before joining").await,
        "must join the channel before updating messages"
    );

    let cid = a
        .send_event("channel_join", json!({"channel_id": "2...lobby-db"}))
        .await;
    a.take_reply(&cid, Some("channel"), "join the db channel")
        .await;
    let cid = b
        .send_event("channel_join", json!({"channel_id": "2...lobby-db"}))
        .await;
    b.take_reply(&cid, Some("channel"), "join the db channel (b)")
        .await;

    let cid = a
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby-db", "content": {"body": "stored"}}),
        )
        .await;
    let ack = a
        .take_reply(&cid, Some("channel_message_ack"), "persistent message ack")
        .await;
    assert_eq!(ack["code"], 0);
    let message_id = ack["message_id"].as_str().expect("message id").to_owned();
    let stored = count(
        env.db(),
        "message",
        Some(
            "stream_mode = 2 \
             AND (stream_subject = <uuid>$nil OR stream_subject = $nil) \
             AND (stream_descriptor = <uuid>$nil OR stream_descriptor = $nil) \
             AND stream_label = $label",
        ),
        &[(
            "nil",
            json!(Uuid::nil().to_string()),
        ), ("label", json!("lobby-db"))],
    )
    .await;
    assert!(
        stored >= 1,
        "the persistent message is stored: {message_id}"
    );

    let message = b.take_event("channel_message").await;
    assert_eq!(message["code"], 0);

    let cid = a
        .send_event(
            "channel_message_update",
            json!({"channel_id": "2...lobby-db", "message_id": message_id.clone(), "content": {"body": "edited"}}),
        )
        .await;
    let updated = a
        .take_reply(&cid, Some("channel_message_ack"), "update ack")
        .await;
    assert_eq!(updated["code"], 1);
    b.take_event("channel_message").await;

    let cid = a
        .send_event(
            "channel_message_remove",
            json!({"channel_id": "2...lobby-db", "message_id": message_id}),
        )
        .await;
    let removed = a
        .take_reply(&cid, Some("channel_message_ack"), "remove ack")
        .await;
    assert_eq!(removed["code"], 2);

    let cid = a
        .send_event(
            "channel_message_update",
            json!({"channel_id": "2...lobby-db", "message_id": "not-a-uuid", "content": "x"}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "update with a malformed id").await,
        "Invalid message ID."
    );

    let cid = a
        .send_event("channel_leave", json!({"channel_id": "2...lobby-db"}))
        .await;
    a.take_reply(&cid, Some("channel_leave"), "leave the db channel")
        .await;
    let cid = b
        .send_event("channel_leave", json!({"channel_id": "2...lobby-db"}))
        .await;
    b.take_reply(&cid, Some("channel_leave"), "leave the db channel (b)")
        .await;
}
