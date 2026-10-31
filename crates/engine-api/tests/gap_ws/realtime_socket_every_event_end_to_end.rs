use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn realtime_socket_every_event_end_to_end() {
    let env = Harness::start().await;

    let alice = device_player(&env.client, &env.base, "ws-al").await;
    let bob = device_player(&env.client, &env.base, "ws-bb").await;
    let carol = device_player(&env.client, &env.base, "ws-cr").await;

    let mut a = WsClient::connect(
        env.port,
        &mint(uid_of(&alice), &alice.username),
        "a",
        uid_of(&alice),
        &alice.username,
    )
    .await;
    let mut b = WsClient::connect(
        env.port,
        &mint(uid_of(&bob), &bob.username),
        "b",
        uid_of(&bob),
        &bob.username,
    )
    .await;

    let created_id = match_plane(&mut a, &mut b).await;

    let list = json_body(
        env.client
            .get(format!("{}/v2/match", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/match lists relay matches",
    )
    .await;
    let ids: Vec<&str> = list["matches"]
        .as_array()
        .expect("the registry is populated after the match plane")
        .iter()
        .filter_map(|m| m["match_id"].as_str())
        .collect();
    assert!(
        ids.contains(&created_id.as_str()),
        "the created match is listed: {ids:?}"
    );
    let label_only = json_body(
        env.client
            .get(format!("{}/v2/match?min_size=1", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/match filters by size",
    )
    .await;
    assert!(
        !label_only["matches"].as_array().unwrap().is_empty(),
        "at least the label match still has a member"
    );
    refusal(
        env.client
            .get(format!("{}/v2/match?limit=0", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "GET /v2/match validates limit",
    )
    .await;
    let stats = json_body(
        env.client
            .get(format!("{}/v2/matchmaker/stats", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/matchmaker/stats",
    )
    .await;
    assert_eq!(stats["ticket_count"], 0, "no tickets are queued yet");

    matchmaker_plane(&mut a, &mut b).await;
    party_plane(&mut a, &mut b).await;
    chat_plane(&mut a, &mut b).await;
    presence_plane(&mut a, &mut b).await;
    channel_messages_plane(&env, &mut a, &mut b).await;

    let mut c = WsClient::connect(
        env.port,
        &mint(uid_of(&carol), &carol.username),
        "c",
        uid_of(&carol),
        &carol.username,
    )
    .await;
    notifications_plane(&env, &mut c, &carol).await;

    c.close().await;
    b.close().await;
    a.close().await;
    env.stop().await;
    eprintln!("ws-e2e: every /ws event tag passed");
}
