use super::*;

pub fn normalized_sid(sid: &str) -> String {
    Uuid::parse_str(sid)
        .map(|uuid| uuid.to_string())
        .unwrap_or_else(|_| sid.to_owned())
}
pub async fn match_plane(a: &mut WsClient, b: &mut WsClient) -> String {
    eprintln!("ws-e2e: match plane");

    let cid = a
        .send_event(
            "match_create",
            json!({"name": "room-e2e-a", "open": true, "tick_rate": 30, "label": "room-e2e-a"}),
        )
        .await;
    let ack = a
        .take_reply(&cid, Some("match_create_ack"), "match create")
        .await;
    let match_id = ack["match_id"].as_str().expect("match id").to_owned();

    let cid_seat = a
        .send_event("match_join", json!({"match_id": match_id.clone()}))
        .await;
    let seated = a
        .take_reply(&cid_seat, Some("match_join"), "seat the creator")
        .await;
    assert_eq!(seated["match_id"].as_str(), Some(match_id.as_str()));
    assert!(
        seated.get("presences").is_none(),
        "an empty match answers the first join with no presences: {seated}"
    );

    let cid_join = b
        .send_event("match_join", json!({"match_id": match_id.clone()}))
        .await;
    let joined = b
        .take_reply(&cid_join, Some("match_join"), "join by id")
        .await;
    assert_eq!(joined["match_id"].as_str(), Some(match_id.as_str()));
    assert_eq!(
        joined["presences"]
            .as_array()
            .map(|rows| rows.len())
            .unwrap_or_default(),
        1,
        "the joiner is told who was already in the match: {joined}"
    );
    let presence = a.take_event("match_presence_event").await;
    assert_eq!(presence["match_id"].as_str(), Some(match_id.as_str()));
    assert!(
        !presence["joins"]
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .is_empty(),
        "the resident hears about the join: {presence}"
    );

    a.fire(
        "match_data_send",
        json!({"match_id": match_id.clone(), "op_code": 3, "data": "hello-b"}),
    )
    .await;
    let data = b.take_event("match_data").await;
    assert_eq!(data["op_code"], 3);
    assert_eq!(data["data"], "hello-b");
    assert_eq!(data["match_id"].as_str(), Some(match_id.as_str()));
    a.expect_quiet_for(Duration::from_millis(300), "no ack to the data sender")
        .await;

    let cid_sig = a
        .send_event(
            "match_signal",
            json!({"match_id": match_id.clone(), "data": "{\"offer\":\"sdp\"}"}),
        )
        .await;
    let echoed = a
        .take_reply(&cid_sig, Some("match_signal"), "signal echo")
        .await;
    let relayed = b.take_event("match_signal").await;
    assert_eq!(relayed["data"], "{\"offer\":\"sdp\"}");
    assert!(
        echoed["data"].as_str().unwrap_or("").is_empty(),
        "the sender's echo is empty: {echoed}"
    );

    let cid_leave = a
        .send_event("match_leave", json!({"match_id": match_id.clone()}))
        .await;
    let left = a.take_reply(&cid_leave, Some("match_leave"), "leave").await;
    assert_eq!(left["match_id"].as_str(), Some(match_id.as_str()));
    let gone = b.take_event("match_presence_event").await;
    assert!(
        !gone["leaves"]
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .is_empty(),
        "the peer hears the leave: {gone}"
    );
    let cid_after = a
        .send_event(
            "match_data_send",
            json!({"match_id": match_id.clone(), "op_code": 1, "data": "late"}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid_after, "data after leave").await,
        "must join the match before sending data"
    );

    a.drain(Duration::from_millis(100)).await;
    let cid_unknown = b
        .send_event(
            "match_data_send",
            json!({"match_id": Uuid::new_v4().to_string(), "op_code": 1, "data": "x"}),
        )
        .await;
    assert_eq!(
        b.take_ws_error(&cid_unknown, "data on unknown match").await,
        "match not found"
    );
    let cid_bad_token = b
        .send_event("match_join", json!({"token": "deadbeefdeadbeef"}))
        .await;
    assert_eq!(
        b.take_ws_error(&cid_bad_token, "join with an unknown token")
            .await,
        "match token invalid or expired"
    );
    let cid_no_selector = b.send_event("match_join", json!({})).await;
    assert_eq!(
        b.take_ws_error(&cid_no_selector, "join with no selector")
            .await,
        "match_join needs match_id, token or label"
    );
    let cid_bad_id = b
        .send_event("match_leave", json!({"match_id": "not-a-uuid"}))
        .await;
    assert_eq!(
        b.take_ws_error(&cid_bad_id, "leave with a malformed id")
            .await,
        "match not found"
    );

    let label = format!("e2e label {}", letters_suffix());
    let cid_label = b
        .send_event("match_join", json!({"label": label.clone()}))
        .await;
    let by_label = b
        .take_reply(&cid_label, Some("match_join"), "join by label")
        .await;
    assert!(
        by_label["match_id"].is_string(),
        "a label join resolves to a match: {by_label}"
    );
    match_id
}
