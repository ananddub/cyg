use super::*;

pub async fn group_surface(env: &Harness) {
    eprintln!("gap: group membership verbs");
    let lead = device_player(&env.client, &env.base, "lead").await;
    let one = device_player(&env.client, &env.base, "one").await;
    let two = device_player(&env.client, &env.base, "two").await;
    let three = device_player(&env.client, &env.base, "three").await;
    let stray = device_player(&env.client, &env.base, "stray").await;

    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "name": format!("e2e ladder {}", letters_suffix()),
                "open": true,
                "max_count": 10,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the ladder group",
    )
    .await;
    let gid = created["group"]["id"].as_str().expect("group id").to_owned();

    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/add?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/add",
    )
    .await;
    members_call(
        env,
        &lead.token,
        &format!(
            "/v2/group/{gid}/add?user_ids={},{}",
            two.uid, three.uid
        ),
        "add two more members in one call",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, Some(2));
    assert_eq!(member_state(env, &lead.token, &gid, &lead.uid).await, Some(0));

    expect(
        env.client
            .post(format!("{}/v2/group/{gid}/add", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "add with no targets is a no-op",
    )
    .await;

    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/promote?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/promote",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, Some(1));
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/demote?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/demote",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, Some(2));

    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/group/{}/add?user_ids={}",
                    env.base, gid, stray.uid
                ))
                .bearer_auth(&two.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a member trying to add",
        )
        .await,
        "Group not found or permission denied."
    );

    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/kick?user_ids={}", two.uid),
        "POST /v2/group/{groupId}/kick",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &two.uid).await, None);
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/ban?user_ids={}", one.uid),
        "POST /v2/group/{groupId}/ban",
    )
    .await;
    assert!(
        member_in_state(env, &lead.token, &gid, &one.uid, 4).await,
        "the ban landed in the banned bucket"
    );
    assert_eq!(
        member_state(env, &lead.token, &gid, &one.uid).await,
        None,
        "the unbucketed listing hides the banned member"
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/join", env.base))
                .bearer_auth(&one.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a banned player rejoining",
        )
        .await,
        "Banned from the group."
    );
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/add?user_ids={}", stray.uid),
        "add the stray as a member",
    )
    .await;
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{gid}/promote?user_ids={}", stray.uid),
        "promote the stray to admin",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/group/{}/kick?user_ids={}",
                    env.base, gid, lead.uid
                ))
                .bearer_auth(&stray.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an admin kicking the superadmin",
        )
        .await,
        "Cannot remove a user of equal or higher role."
    );

    members_call(
        env,
        &one.token,
        &format!("/v2/group/{gid}/leave"),
        "POST /v2/group/{groupId}/leave",
    )
    .await;
    assert_eq!(member_state(env, &lead.token, &gid, &one.uid).await, None);
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/leave", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the last superadmin trying to leave",
        )
        .await,
        "Cannot leave group when you are the last superadmin."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/leave", env.base))
                .bearer_auth(&one.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "leaving a group you are not in",
        )
        .await,
        "Group not found."
    );

    let walker = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "name": format!("e2e walker {}", letters_suffix()),
                "open": true,
                "max_count": 10,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the paging group",
    )
    .await;
    let walk_gid = walker["group"]["id"].as_str().expect("group id").to_owned();
    let mut walked = Vec::new();
    for label in ["w1x", "w2x", "w3x"] {
        walked.push(device_player(&env.client, &env.base, label).await);
    }
    for m in &walked {
        members_call(
            env,
            &lead.token,
            &format!("/v2/group/{walk_gid}/add?user_ids={}", m.uid),
            "add a paging member",
        )
        .await;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = String::new();
    for _ in 0..8 {
        let url = if cursor.is_empty() {
            format!("{}/v2/group/{}/user?state=2&limit=1", env.base, walk_gid)
        } else {
            format!(
                "{}/v2/group/{}/user?state=2&limit=1&cursor={}",
                env.base, walk_gid, cursor
            )
        };
        let page = json_body(
            env.client
                .get(url)
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "GET /v2/group/{groupId}/user?state=2&limit=1&cursor= (paging)",
        )
        .await;
        for row in page["group_users"].as_array().expect("a page").clone() {
            seen.push(row["user"]["id"].as_str().expect("member id").to_owned());
        }
        cursor = page["cursor"].as_str().unwrap_or_default().to_owned();
        if cursor.is_empty() {
            break;
        }
    }
    assert_eq!(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        seen.len(),
        "no member repeats across the state-filtered pages: {seen:?}"
    );
    assert_eq!(
        seen.len(),
        3,
        "the walk saw each of the three members once: {seen:?}"
    );
    for m in &walked {
        assert!(
            seen.contains(&m.uid),
            "the cursor walk saw every member: {seen:?}"
        );
    }
    let members = json_body(
        env.client
            .get(format!("{}/v2/group/{gid}/user?state=2&limit=10", env.base))
            .bearer_auth(&lead.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{groupId}/user?state=",
    )
    .await;
    let rows = members["group_users"].as_array().expect("a page").clone();
    assert!(
        rows.iter().all(|row| row["state"] == 2) && !rows.is_empty(),
        "the state filter keeps only members: {members}"
    );
    for (label, query, status, message) in [
        ("limit=0", "limit=0", StatusCode::BAD_REQUEST, "Invalid limit - limit must be between 1 and 100."),
        ("limit=101", "limit=101", StatusCode::BAD_REQUEST, "Invalid limit - limit must be between 1 and 100."),
        ("state=5", "state=5", StatusCode::BAD_REQUEST, "Invalid state - state must be between 0 and 4."),
        ("cursor=not-a-cursor", "cursor=not-a-cursor", StatusCode::BAD_REQUEST, "Cursor is invalid."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .get(format!("{}/v2/group/{gid}/user?{query}", env.base, query = query))
                    .bearer_auth(&lead.token)
                    .send()
                    .await
                    .unwrap(),
                status,
                &format!("the user listing refuses {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/group/{gid}/user", env.base))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "the user listing with no session",
        )
        .await,
        "Auth token required"
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/not-a-uuid/add", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an add against a malformed group id",
        )
        .await,
        "Group ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/add?user_ids=not-a-uuid", env.base))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an add aimed at a malformed user id",
        )
        .await,
        "User ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/group/{gid}/ban?user_ids={}", env.base, Uuid::new_v4()))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a ban aimed at an account that does not exist",
        )
        .await,
        "One or more users not found."
    );
    let ghost = Uuid::new_v4().to_string();
    for (label, path) in [
        ("add", format!("/v2/group/{ghost}/add")),
        ("kick", format!("/v2/group/{ghost}/kick")),
        ("ban", format!("/v2/group/{ghost}/ban")),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(format!("{}{}", env.base, path))
                    .bearer_auth(&lead.token)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::NOT_FOUND,
                &format!("a {label} against a group that does not exist"),
            )
            .await,
            "Group not found.",
            "{label}"
        );
    }

    let packed = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&lead.token)
            .json(&json!({
                "name": format!("e2e packed {}", letters_suffix()),
                "open": true,
                "max_count": 2,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create the two-seat group",
    )
    .await;
    let packed_id = packed["group"]["id"].as_str().expect("group id").to_owned();
    members_call(
        env,
        &lead.token,
        &format!("/v2/group/{packed_id}/add?user_ids={}", three.uid),
        "fill the first free seat",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/group/{}/add?user_ids={}",
                    env.base, packed_id, stray.uid
                ))
                .bearer_auth(&lead.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding past the cap",
        )
        .await,
        "Group is full."
    );
}
