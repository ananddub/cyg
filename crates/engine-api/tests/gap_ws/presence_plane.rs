use super::*;

pub async fn presence_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: presence plane");

    let cid = a
        .send_event("status_follow", json!({"user_ids": [b.uid.to_string()]}))
        .await;
    let followed = a
        .take_reply(&cid, Some("status_followed"), "follow by id")
        .await;
    assert_eq!(
        followed["user_ids"].as_array().map(Vec::len),
        Some(1),
        "the followed id comes back: {followed}"
    );

    b.fire("status_update", json!({"status": "in-game"})).await;
    let event = a.take_event("status_presence_event").await;
    assert_eq!(event["joins"].as_array().map(Vec::len), Some(1));
    assert_eq!(event["joins"][0]["status"], "in-game");
    b.expect_quiet_for(Duration::from_millis(200), "no reply to a status update")
        .await;

    let cid = a
        .send_event("status_follow", json!({"usernames": ["nobody-e2e"]}))
        .await;
    let followed = a
        .take_reply(&cid, Some("status_followed"), "follow an unknown name")
        .await;
    assert_eq!(
        followed["user_ids"].as_array().map(Vec::len),
        Some(0),
        "nothing resolves: {followed}"
    );
    b.fire("status_update", json!({"status": "queued"})).await;
    let held = a.take_event("status_presence_event").await;
    assert_eq!(held["joins"][0]["status"], "queued");
    assert!(
        held.get("cid").is_none(),
        "a fan-out carries no request cid: {held}"
    );

    let cid = a
        .send_event("status_unfollow", json!({"user_ids": [b.uid.to_string()]}))
        .await;
    let unfollowed = a
        .take_reply(&cid, Some("status_followed"), "unfollow")
        .await;
    assert_eq!(unfollowed["user_ids"].as_array().map(Vec::len), Some(0));
    b.fire("status_update", json!({"status": "idle"})).await;
    a.expect_quiet_for(Duration::from_millis(400), "no events after unfollow")
        .await;
}
pub async fn notifications_plane(env: &Harness, a: &mut WsClient, player: &Player) {
    eprintln!("ws-e2e: notifications plane");
    let note_id = Uuid::new_v4();
    exec(
        env.db(),
        "CREATE type::record('notification', <uuid>$id) CONTENT $row RETURN NOTHING",
        &[
            ("id", json!(note_id.to_string())),
            (
                "row",
                json!({
                    "user_id": uid_of(player).to_string(),
                    "subject": "ws-e2e notification",
                    "content": {},
                    "code": 7,
                    "sender_id": Uuid::new_v4().to_string(),
                    "create_time": engine_api::db::time::encode(chrono::Utc::now()),
                }),
            ),
        ],
    )
    .await;

    let cid = a.send_event("notifications", json!({"limit": 50})).await;
    let list = a.take_reply(&cid, None, "notifications list").await;
    assert_eq!(
        list["type"], "notifications_list",
        "the reply tag differs from the request tag"
    );
    let simple = note_id.simple().to_string();
    let hit = list["notifications"]
        .as_array()
        .map(|rows| rows.as_slice())
        .unwrap_or(&[])
        .iter()
        .find(|n| {
            n["id"]
                .as_str()
                .unwrap_or_default()
                .replace('-', "")
                .eq_ignore_ascii_case(&simple)
        })
        .unwrap_or_else(|| panic!("the seeded notification is listed: {list}"));
    assert_eq!(hit["subject"], "ws-e2e notification");
    assert_eq!(hit["code"], 7);
}
