use super::*;

pub async fn party_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: party plane");

    let cid = a
        .send_event("party_create", json!({"max_size": 3, "open": true}))
        .await;
    let created = a.take_reply(&cid, Some("party"), "party create").await;
    let party_id = created["party"]["party_id"]
        .as_str()
        .expect("party id")
        .to_owned();
    let leader_sid = normalized_sid(
        created["party"]["leader"]["session_id"]
            .as_str()
            .expect("leader session id"),
    );

    a.drain(Duration::from_millis(100)).await;
    let cid_join = b
        .send_event("party_join", json!({"party_id": party_id.clone()}))
        .await;
    let joined = b.take_reply(&cid_join, Some("party"), "party join").await;
    assert_eq!(
        joined["party"]["party_id"].as_str(),
        Some(party_id.as_str())
    );
    let member_event = a.take_event("party_member_join").await;
    assert_eq!(member_event["party_id"].as_str(), Some(party_id.as_str()));
    let joiner_sid = normalized_sid(
        member_event["joins"][0]["session_id"]
            .as_str()
            .expect("the joiner's session id"),
    );

    b.fire(
        "party_data_send",
        json!({"party_id": party_id.clone(), "op_code": 5, "data": "hi"}),
    )
    .await;
    let data = a.take_event("party_data").await;
    assert_eq!(data["op_code"], 5);
    assert_eq!(data["data"], "hi");

    let cid_promote = b
        .send_event(
            "party_promote",
            json!({"party_id": party_id.clone(), "presence": {"user_id": a.uid.to_string(), "session_id": leader_sid.clone(), "username": a.user}}),
        )
        .await;
    assert_eq!(
        b.take_ws_error(&cid_promote, "promotion by a non-leader")
            .await,
        "only party leader can promote"
    );
    b.drain(Duration::from_millis(50)).await;
    a.fire(
        "party_promote",
        json!({"party_id": party_id.clone(), "presence": {"user_id": b.uid.to_string(), "session_id": joiner_sid.clone(), "username": b.user}}),
    )
    .await;
    a.drain(Duration::from_millis(100)).await;
    let promoted_b = b.take_event("party_promote").await;
    assert_eq!(promoted_b["party_id"].as_str(), Some(party_id.as_str()));
    assert_eq!(
        normalized_sid(
            promoted_b["presence"]["session_id"]
                .as_str()
                .expect("promoted session id")
        ),
        joiner_sid,
        "the new leader is announced"
    );

    let cid_update = a
        .send_event(
            "party_update",
            json!({"party_id": party_id.clone(), "max_size": 2}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid_update, "update by the demoted ex-leader")
            .await,
        "only party leader can update party"
    );
    b.fire(
        "party_update",
        json!({"party_id": party_id.clone(), "open": true}),
    )
    .await;
    b.expect_quiet_for(Duration::from_millis(300), "a silent successful update")
        .await;
    a.drain(Duration::from_millis(100)).await;

    b.fire(
        "party_close",
        json!({"party_id": party_id.clone(), "metadata": {"bye": true}}),
    )
    .await;
    let closed_a = a.take_event("party_close").await;
    let closed_b = b.take_event("party_close").await;
    assert_eq!(closed_a["metadata"]["bye"], true);
    assert_eq!(closed_b["type"], "party_close");

    b.fire(
        "party_data_send",
        json!({"party_id": party_id.clone(), "data": "after close"}),
    )
    .await;
    b.drain(Duration::from_millis(200)).await;
    a.drain(Duration::from_millis(200)).await;

    let cid = a
        .send_event(
            "party_matchmaker_add",
            json!({"min_count": 2, "max_count": 2}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "party ticket with no party").await,
        "not a member of a party"
    );
    let cid = a
        .send_event(
            "party_matchmaker_remove",
            json!({"party_id": Uuid::new_v4().to_string(), "ticket": Uuid::new_v4().simple().to_string()}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "party ticket remove outside a party")
            .await,
        "not a member of this party"
    );

    let cid = a
        .send_event("party_create", json!({"max_size": 2, "open": false}))
        .await;
    let created = a
        .take_reply(&cid, Some("party"), "private party create")
        .await;
    let private_id = created["party"]["party_id"]
        .as_str()
        .expect("private party id")
        .to_owned();
    a.drain(Duration::from_millis(100)).await;
    let cid_join = b
        .send_event("party_join", json!({"party_id": private_id.clone()}))
        .await;
    assert_eq!(
        b.take_ws_error(&cid_join, "joining a private party").await,
        "party is private"
    );
    let cid_close = a
        .send_event("party_close", json!({"party_id": private_id}))
        .await;
    a.take_event("party_close").await;
    a.drain(Duration::from_millis(100)).await;
    let _ = cid_close;

    b.fire(
        "party_leave",
        json!({"party_id": Uuid::new_v4().to_string()}),
    )
    .await;
    b.drain(Duration::from_millis(200)).await;
    a.drain(Duration::from_millis(100)).await;
    b.drain(Duration::from_millis(100)).await;
}
