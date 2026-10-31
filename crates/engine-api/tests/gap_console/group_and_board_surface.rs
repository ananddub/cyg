use super::*;

pub async fn group_and_board_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console group roster and board records");
    let leader = device_player(&env.client, &env.base, "gldr").await;
    let member = device_player(&env.client, &env.base, "gmb").await;
    let name = format!("e2e roster {}", letters_suffix());
    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&leader.token)
            .json(&json!({ "name": name, "open": true, "max_count": 8 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a group",
    )
    .await;
    let group_id = created["group"]["id"]
        .as_str()
        .expect("group id")
        .to_owned();
    expect(
        env.client
            .post(format!("{}/v2/group/{group_id}/join", env.base))
            .bearer_auth(&member.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the second player joins",
    )
    .await;

    let roster = json_body(
        env.client
            .get(format!("{}/v2/console/group/{group_id}/member", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/group/{id}/member",
    )
    .await;
    let rows = roster.as_array().expect("an array of members").clone();
    assert_eq!(rows.len(), 2, "{roster}");
    let states: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["username"].as_str().unwrap_or_default(),
                row["state"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        states.contains(&(leader.username.as_str(), "superadmin")),
        "the creator leads: {states:?}"
    );
    assert!(
        states.contains(&(member.username.as_str(), "member")),
        "the joiner is a member: {states:?}"
    );

    let board = format!("e2e_console_board_{}", letters_suffix());
    seed_leaderboard(env.db(), &board, false).await;
    for (token, score) in [(&leader.token, 300), (&member.token, 900)] {
        json_body(
            env.client
                .post(format!("{}/v2/leaderboard/{board}", env.base))
                .bearer_auth(token)
                .json(&json!({ "score": score }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "write a record",
        )
        .await;
    }

    let records = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/{}/records",
                env.base, board
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/leaderboard/{id}/records",
    )
    .await;
    let rows = records.as_array().expect("an array of records").clone();
    assert_eq!(rows.len(), 2, "{records}");
    assert_eq!(rows[0]["leaderboard_id"], board);
    assert_eq!(
        rows[0]["score"].as_i64(),
        Some(900),
        "the board reads in score order"
    );
    let unknown = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/not-a-uuid/records",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reading records of a board that does not exist",
    )
    .await;
    assert_eq!(unknown, json!([]), "an unknown board lists nothing");

    let owned = create_game(env, console, "Gap Board Game", "custom").await;
    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/v2/console/leaderboard/{}/records?game_id={}",
                    env.base, board, owned.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "reading a board through a game that does not own it",
        )
        .await,
        "Leaderboard not found."
    );
    exec(
        env.db(),
        "UPDATE type::record('leaderboard', $id) SET game_id = $game RETURN NOTHING",
        &[("id", json!(board)), ("game", json!(owned.id))],
    )
    .await;
    let scoped = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/{}/records?game_id={}",
                env.base, board, owned.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reading the records of the board this game owns",
    )
    .await;
    assert_eq!(scoped.as_array().map(Vec::len), Some(2), "{scoped}");
    let other = create_game(env, console, "Gap Other Game", "custom").await;
    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/v2/console/leaderboard/{}/records?game_id={}",
                    env.base, board, other.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "reading a board through the wrong game",
        )
        .await,
        "Leaderboard not found."
    );

    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/leaderboard/{}/owner/not-a-uuid",
                    env.base, board
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "removing the record of an account that is not a uuid",
        )
        .await,
        "Invalid owner ID."
    );
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/leaderboard/{}/owner/{}",
                env.base, board, member.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/leaderboard/{id}/owner/{ownerId}",
    )
    .await;
    let after = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/{}/records",
                env.base, board
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the board after the console removed one record",
    )
    .await;
    let left = after.as_array().expect("an array of records").clone();
    assert_eq!(left.len(), 1, "{after}");
    assert_eq!(
        left[0]["owner_id"], leader.uid,
        "the leader's score survives"
    );

    refusal(
        env.client
            .delete(format!("{}/v2/console/leaderboard/{}", env.base, board))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "deleting a board as a read-only operator",
    )
    .await;
    expect(
        env.client
            .delete(format!("{}/v2/console/leaderboard/{}", env.base, board))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/leaderboard/{id}",
    )
    .await;
}
