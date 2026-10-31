use super::*;

pub async fn group_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: groups");
    let owner = device_player(client, base, "gr-o").await;
    let member1 = device_player(client, base, "gr-m").await;
    let member2 = device_player(client, base, "gr-n").await;
    let outsider = device_player(client, base, "gr-x").await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group without a name",
        )
        .await,
        "Group name must be set."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": format!("e2e-bad-{}", letters_suffix()), "max_count": -1 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group with a negative size",
        )
        .await,
        "Group max count must be >= 1 when set."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({
                    "name": format!("e2e-bad-{}", letters_suffix()),
                    "creator_id": Uuid::new_v4().to_string(),
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group for another creator",
        )
        .await,
        "Group creator invalid."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": format!("e2e-bad-{}", letters_suffix()), "metadata": "{not json" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group with bad metadata",
        )
        .await,
        "Failed to unmarshal group metadata."
    );

    let open_name = format!("e2e-open-{}", letters_suffix());
    let created = api_post(
        client,
        format!("{base}/v2/group"),
        &owner.token,
        json!({
            "name": open_name,
            "open": true,
            "max_count": 2,
            "description": "the open group",
            "lang_tag": "en",
            "metadata": r#"{"tier":"gold"}"#,
        }),
        "create an open group",
    )
    .await;
    let open_id = created["group"]["id"].as_str().expect("group id").to_owned();
    let open = Uuid::parse_str(&open_id).expect("group uuid");
    assert_eq!(created["group"]["creator_id"], owner.uid);
    assert_eq!(created["group"]["name"], open_name);
    assert_eq!(created["group"]["open"], true);
    assert_eq!(created["group"]["edge_count"], 1);
    assert_eq!(created["group"]["max_count"], 2);
    assert_eq!(created["group"]["description"], "the open group");
    assert_eq!(created["group"]["lang_tag"], "en");
    assert_eq!(
        json_string(&created["group"]["metadata"]),
        json!({ "tier": "gold" })
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": open_name }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "reusing a group name",
        )
        .await,
        "Group name is in use."
    );

    let fetched = api_get(
        client,
        format!("{base}/v2/group/{open_id}"),
        &owner.token,
        "get a group",
    )
    .await;
    assert_eq!(fetched["group"]["id"], open_id);
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/not-a-uuid"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "getting a malformed group id",
        )
        .await,
        "Group ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{}", Uuid::new_v4()))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "getting an unknown group",
        )
        .await,
        "Group not found."
    );

    let by_name = api_get(
        client,
        with_query(&format!("{base}/v2/group"), &[("name", open_name.as_str())]),
        &owner.token,
        "searching groups by name",
    )
    .await;
    assert_eq!(by_name["groups"][0]["id"], open_id);
    assert_eq!(
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/group"),
                    &[("name", open_name.as_str()), ("open", "true")],
                ))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "combining the name filter with another filter",
        )
        .await,
        "name filter cannot be combined with any other filter"
    );

    let discover = api_get(
        client,
        format!("{base}/v2/group?open=true&members=1"),
        &owner.token,
        "discovering open groups",
    )
    .await;
    assert!(discover["groups"]
        .as_array()
        .expect("group list")
        .iter()
        .any(|group| group["id"] == open_id));
    assert!(discover["groups"].as_array().unwrap().len() <= 1);

    let bad_bool = refusal(
        client
            .get(format!("{base}/v2/group?open=maybe"))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "an unparseable open filter",
    )
    .await;
    assert!(bad_bool.starts_with("invalid query parameter"), "{bad_bool}");
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group?limit=0"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group list limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group?cursor=not-a-cursor"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group list bad cursor",
        )
        .await,
        "Cursor is invalid."
    );

    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/join"),
            &member1.token,
            "member1 joins the open group",
        )
        .await,
        json!({})
    );
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(2));
    assert_eq!(group_count(pool, open).await, 2);
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/join"))
                .bearer_auth(&member2.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "joining a full group",
        )
        .await,
        "Group is full."
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/leave"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the last superadmin leaving",
        )
        .await,
        "Cannot leave group when you are the last superadmin."
    );
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/leave"),
            &member1.token,
            "member1 leaves the open group",
        )
        .await,
        json!({})
    );
    assert_eq!(group_count(pool, open).await, 1);
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, None);

    api_post_empty(
        client,
        format!("{base}/v2/group/{open_id}/join"),
        &member2.token,
        "member2 joins the freed slot",
    )
    .await;
    assert_eq!(group_edge_state(pool, uuid_of(&member2), open).await, Some(2));
    assert_eq!(group_count(pool, open).await, 2);

    assert_eq!(
        api_post_empty(
            client,
            format!(
                "{base}/v2/group/{open_id}/ban?user_ids={}",
                member2.uid
            ),
            &owner.token,
            "owner bans member2",
        )
        .await,
        json!({})
    );
    assert_eq!(
        group_edge_state(pool, uuid_of(&member2), open).await,
        Some(4)
    );
    assert_eq!(group_count(pool, open).await, 1);
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/join"))
                .bearer_auth(&member2.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a banned player rejoining",
        )
        .await,
        "Banned from the group."
    );

    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/add"),
            &owner.token,
            "adding no users",
        )
        .await,
        json!({})
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/add?user_ids=not-a-uuid"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding a malformed user id",
        )
        .await,
        "User ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            client
                .post(format!(
                    "{base}/v2/group/{open_id}/add?user_ids={}",
                    Uuid::new_v4()
                ))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding an unknown user",
        )
        .await,
        "One or more users not found."
    );
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/add?user_ids={}", member1.uid),
            &owner.token,
            "owner adds member1",
        )
        .await,
        json!({})
    );
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(2));
    assert_eq!(group_count(pool, open).await, 2);

    let roster = api_get(
        client,
        format!("{base}/v2/group/{open_id}/user?limit=100"),
        &owner.token,
        "the group roster",
    )
    .await;
    let roster = roster["group_users"].as_array().expect("a roster");
    assert_eq!(roster.len(), 2);
    assert_eq!(roster[0]["user"]["id"], owner.uid);
    assert_eq!(roster[0]["state"], 0);
    assert_eq!(roster[1]["user"]["id"], member1.uid);
    assert_eq!(roster[1]["state"], 2);

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}/user?state=5"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group roster state out of range",
        )
        .await,
        "Invalid state - state must be between 0 and 4."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}/user?limit=0"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group roster limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}/user?cursor=not-a-cursor"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group roster bad cursor",
        )
        .await,
        "Cursor is invalid."
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{open_id}/promote?user_ids={}", member1.uid),
        &owner.token,
        "owner promotes member1",
    )
    .await;
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(1));
    let admins = api_get(
        client,
        format!("{base}/v2/group/{open_id}/user?state=1"),
        &owner.token,
        "the group's admins",
    )
    .await;
    let admins = admins["group_users"].as_array().expect("an admin page");
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0]["user"]["id"], member1.uid);

    assert_eq!(
        refusal(
            client
                .post(format!(
                    "{base}/v2/group/{open_id}/kick?user_ids={}",
                    owner.uid
                ))
                .bearer_auth(&member1.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an admin kicking the superadmin",
        )
        .await,
        "Cannot remove a user of equal or higher role."
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{open_id}/demote?user_ids={}", member1.uid),
        &owner.token,
        "owner demotes member1",
    )
    .await;
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(2));

    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/kick?user_ids={}", member1.uid),
            &owner.token,
            "owner kicks member1",
        )
        .await,
        json!({})
    );
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, None);
    assert_eq!(group_count(pool, open).await, 1);
    let roster = api_get(
        client,
        format!("{base}/v2/group/{open_id}/user?limit=100"),
        &owner.token,
        "the roster after the kick",
    )
    .await;
    assert_eq!(roster["group_users"].as_array().unwrap().len(), 1);

    let closed_name = format!("e2e-closed-{}", letters_suffix());
    let closed = api_post(
        client,
        format!("{base}/v2/group"),
        &owner.token,
        json!({ "name": closed_name }),
        "create a closed group",
    )
    .await;
    let closed_id = closed["group"]["id"]
        .as_str()
        .expect("closed group id")
        .to_owned();
    let closed_uuid = Uuid::parse_str(&closed_id).expect("closed group uuid");
    assert_eq!(closed["group"]["open"], false);
    assert_eq!(closed["group"]["max_count"], 100);
    assert!(closed["group"].get("description").is_none());
    assert!(closed["group"].get("avatar_url").is_none());

    api_post_empty(
        client,
        format!("{base}/v2/group/{closed_id}/join"),
        &outsider.token,
        "outsider requests to join the closed group",
    )
    .await;
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        Some(3)
    );
    assert_eq!(group_count(pool, closed_uuid).await, 1);
    let request = api_get(
        client,
        format!(
            "{base}/v2/notification?limit=10&sender_id={}",
            outsider.uid
        ),
        &owner.token,
        "the owner's join-request notification",
    )
    .await;
    let request = request["notifications"]
        .as_array()
        .expect("the owner has notifications")
        .iter()
        .find(|n| n["code"] == -5)
        .expect("a join-request notification");
    let content = json_string(&request["content"]);
    assert_eq!(content["group_id"], closed_id);
    assert_eq!(content["username"], outsider.username);
    assert_eq!(
        request["subject"],
        format!("User {} wants to join your group", outsider.username)
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{closed_id}/join"),
        &outsider.token,
        "outsider requests again",
    )
    .await;
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        Some(3)
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{closed_id}/add?user_ids={}", outsider.uid),
        &owner.token,
        "owner accepts the join request",
    )
    .await;
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        Some(2)
    );
    assert_eq!(group_count(pool, closed_uuid).await, 2);
    let added = api_get(
        client,
        format!(
            "{base}/v2/notification?limit=10&sender_id={}",
            owner.uid
        ),
        &outsider.token,
        "the added player's notification",
    )
    .await;
    let added = added["notifications"]
        .as_array()
        .expect("the added player has notifications")
        .iter()
        .find(|n| n["code"] == -4)
        .expect("an added-to-group notification");
    assert_eq!(
        added["subject"],
        format!("You've been added to group {closed_name}")
    );
    assert_eq!(json_string(&added["content"])["group_id"], closed_id);

    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an empty group update",
        )
        .await,
        "Specify at least one field to update."
    );
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": open_name }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "renaming a group to a taken name",
        )
        .await,
        "Group name is in use."
    );
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .json(&json!({ "max_count": 0 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "shrinking a group to nothing",
        )
        .await,
        "Group max count must be >= 1 when set."
    );
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&outsider.token)
                .json(&json!({ "description": "not mine" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a member updating a group",
        )
        .await,
        "Group not found or permission denied."
    );
    assert_eq!(
        api_put(
            client,
            format!("{base}/v2/group/{closed_id}"),
            &owner.token,
            json!({ "description": "updated", "open": true }),
            "owner updates the group",
        )
        .await,
        json!({})
    );
    let updated = api_get(
        client,
        format!("{base}/v2/group/{closed_id}"),
        &owner.token,
        "the updated group",
    )
    .await;
    assert_eq!(updated["group"]["description"], "updated");
    assert_eq!(updated["group"]["open"], true);

    let groups = api_get(
        client,
        format!("{base}/v2/user/{}/group", outsider.uid),
        &owner.token,
        "the outsider's groups",
    )
    .await;
    assert!(groups["user_groups"]
        .as_array()
        .expect("a user-group page")
        .iter()
        .any(|entry| entry["group"]["id"] == closed_id && entry["state"] == 2));
    let only_member = api_get(
        client,
        format!("{base}/v2/user/{}/group?state=2", outsider.uid),
        &owner.token,
        "the outsider's memberships",
    )
    .await;
    assert!(only_member["user_groups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["state"] == 2));
    let none = api_get(
        client,
        format!("{base}/v2/user/{}/group?state=0", outsider.uid),
        &owner.token,
        "groups the outsider owns",
    )
    .await;
    assert_eq!(none["user_groups"], json!([]));
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/{}/group?state=5", outsider.uid))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "user-group state out of range",
        )
        .await,
        "Invalid state - state must be between 0 and 4."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/{}/group?limit=0", outsider.uid))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "user-group limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/not-a-uuid/group"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a malformed user id on the group alias",
        )
        .await,
        "Group ID must be a valid ID."
    );

    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{closed_id}/leave"),
            &outsider.token,
            "outsider leaves the closed group",
        )
        .await,
        json!({})
    );
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        None
    );
    assert_eq!(group_count(pool, closed_uuid).await, 1);
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{closed_id}/leave"))
                .bearer_auth(&outsider.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "leaving a group the caller is not in",
        )
        .await,
        "Group not found."
    );

    assert_eq!(
        refusal(
            client
                .delete(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&member1.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a non-member deleting the group",
        )
        .await,
        "Group not found or permission denied."
    );
    assert_eq!(
        api_delete(
            client,
            format!("{base}/v2/group/{closed_id}"),
            &owner.token,
            "owner deletes the closed group",
        )
        .await,
        json!({})
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "getting a deleted group",
        )
        .await,
        "Group not found."
    );
    let owner_groups = api_get(
        client,
        format!("{base}/v2/user/{}/group?limit=100", owner.uid),
        &owner.token,
        "the owner's groups after a delete",
    )
    .await;
    assert!(owner_groups["user_groups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["group"]["id"] != closed_id));

    assert_eq!(
        api_delete(
            client,
            format!("{base}/v2/group/{open_id}"),
            &owner.token,
            "owner deletes the open group",
        )
        .await,
        json!({})
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "getting the deleted open group",
        )
        .await,
        "Group not found."
    );
}
