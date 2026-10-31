use super::*;

pub async fn matchmaker_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: matchmaker plane");
    let props = json!({"mode": "e2ews"});
    let query = json!("+properties.mode:e2ews");
    let cid_a = a
        .send_event(
            "matchmaker_add",
            json!({"min_count": 2, "max_count": 2, "query": query.clone(), "string_properties": props.clone()}),
        )
        .await;
    let ticket = a
        .take_reply(&cid_a, Some("matchmaker_ticket"), "ticket a")
        .await;
    let ticket_a = ticket["ticket"].as_str().expect("ticket a").to_owned();
    assert!(!ticket_a.is_empty());

    let cid_min = a
        .send_event("matchmaker_add", json!({"min_count": 1, "max_count": 9}))
        .await;
    let message = a.take_ws_error(&cid_min, "ticket out of bounds").await;
    assert!(
        message.contains("min_count and max_count"),
        "unexpected bounds refusal: {message}"
    );

    let cid_b = b
        .send_event(
            "matchmaker_add",
            json!({"min_count": 2, "max_count": 2, "query": query, "string_properties": props}),
        )
        .await;
    b.take_reply(&cid_b, Some("matchmaker_ticket"), "ticket b")
        .await;
    let matched_a = a.take_event("matchmaker_matched").await;
    let matched_b = b.take_event("matchmaker_matched").await;
    let match_id = matched_a["match_id"].as_str().expect("matched match id");
    assert_eq!(matched_b["match_id"].as_str(), Some(match_id));

    let token_a = matched_a["token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            matched_a["users"]
                .as_array()
                .map(|rows| rows.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|u| u["presence"]["user_id"].as_str() == Some(&a.uid.simple().to_string()))
                .find_map(|u| u["token"].as_str())
        })
        .expect("matchmaker_matched carries a join token")
        .to_owned();
    let token_b = matched_b["token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            matched_b["users"]
                .as_array()
                .map(|rows| rows.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|u| u["presence"]["user_id"].as_str() == Some(&b.uid.simple().to_string()))
                .find_map(|u| u["token"].as_str())
        })
        .expect("the second side carries its own token")
        .to_owned();

    let cid_join = a.send_event("match_join", json!({"token": token_a})).await;
    let joined = a
        .take_reply(&cid_join, Some("match_join"), "join by token")
        .await;
    assert_eq!(joined["match_id"].as_str(), Some(match_id));
    let cid_join = b.send_event("match_join", json!({"token": token_b})).await;
    b.take_reply(&cid_join, Some("match_join"), "join b by token")
        .await;
    a.drain(Duration::from_millis(200)).await;
    b.drain(Duration::from_millis(200)).await;

    a.fire("match_leave", json!({"match_id": match_id.to_owned()}))
        .await;
    b.fire("match_leave", json!({"match_id": match_id.to_owned()}))
        .await;
    a.drain(Duration::from_millis(300)).await;
    b.drain(Duration::from_millis(300)).await;

    let cid_add = a
        .send_event(
            "matchmaker_add",
            json!({"min_count": 8, "max_count": 8, "query": "+properties.mode:e2ews-solo", "string_properties": {"mode": "e2ews-solo"}}),
        )
        .await;
    let ticket = a
        .take_reply(&cid_add, Some("matchmaker_ticket"), "solo ticket")
        .await;
    let ticket_solo = ticket["ticket"].as_str().expect("ticket").to_owned();
    let cid_remove = a
        .send_event("matchmaker_remove", json!({"ticket": ticket_solo.clone()}))
        .await;
    let done = a
        .take_reply(&cid_remove, Some("matchmaker_done"), "ticket removed")
        .await;
    assert_eq!(done["type"], "matchmaker_done");
    let cid_again = a
        .send_event("matchmaker_remove", json!({"ticket": ticket_solo}))
        .await;
    assert_eq!(
        a.take_ws_error(&cid_again, "removing a gone ticket").await,
        "matchmaker ticket not found"
    );
    let cid_bad = a
        .send_event("matchmaker_remove", json!({"ticket": "nope"}))
        .await;
    assert_eq!(
        a.take_ws_error(&cid_bad, "removing a malformed ticket")
            .await,
        "matchmaker ticket not found"
    );
}
