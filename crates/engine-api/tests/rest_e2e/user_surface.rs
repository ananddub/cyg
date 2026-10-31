use super::*;

pub async fn user_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: user lookups");
    let a = device_player(client, base, "usr").await;
    let b = device_player(client, base, "usrb").await;

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user?ids={}", a.uid))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "user lookup without a token",
        )
        .await,
        "Auth token required"
    );

    let empty = json_body(
        client
            .get(format!("{base}/v2/user"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup with nothing requested",
    )
    .await;
    assert!(empty["users"].as_array().unwrap().is_empty());

    let found = json_body(
        client
            .get(format!("{base}/v2/user?ids={}", a.uid))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup by id",
    )
    .await;
    let users = found["users"].as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["id"], a.uid);
    assert_eq!(users[0]["username"], a.username);
    assert_eq!(users[0]["metadata"], "{}");
    assert_eq!(users[0]["lang_tag"], "en");
    assert!(users[0]["edge_count"].is_null());
    assert!(users[0]["create_time"].as_str().unwrap().ends_with('Z'));

    let both = json_body(
        client
            .get(format!(
                "{base}/v2/user?ids={},{}&usernames={}",
                a.uid, b.uid, b.username
            ))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup by comma-joined keys",
    )
    .await;
    assert_eq!(both["users"].as_array().unwrap().len(), 3);

    let missing = json_body(
        client
            .get(format!(
                "{base}/v2/user?ids={}&usernames=absent{}",
                Uuid::new_v4(),
                letters_suffix()
            ))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup with misses",
    )
    .await;
    assert!(missing["users"].as_array().unwrap().is_empty());

    let bad_id = refusal(
        client
            .get(format!("{base}/v2/user?ids=not-a-uuid"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "user lookup with a malformed id",
    )
    .await;
    assert_eq!(bad_id, "ID 'not-a-uuid' is not a valid system ID.");

    let by_body = json_body(
        client
            .post(format!("{base}/v2/user"))
            .bearer_auth(&a.token)
            .json(&json!({ "ids": [a.uid], "usernames": [b.username] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user multi-get by body",
    )
    .await;
    assert_eq!(by_body["users"].as_array().unwrap().len(), 2);

    let single = json_body(
        client
            .get(format!("{base}/v2/user/{}", a.uid))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "get a user by id",
    )
    .await;
    assert_eq!(single["users"][0]["username"], a.username);
    let by_name = json_body(
        client
            .get(format!("{base}/v2/user/{}", b.username))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "get a user by username",
    )
    .await;
    assert_eq!(by_name["users"][0]["id"], b.uid);
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/absent{}", letters_suffix()))
                .bearer_auth(&a.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "get a user that does not exist",
        )
        .await,
        "User account not found."
    );

    set_disable(pool, &b.uid, &engine_api::db::time::encode(Utc::now())).await;
    let hidden = json_body(
        client
            .get(format!("{base}/v2/user?ids={}", b.uid))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a disabled account is absent from the multi-get",
    )
    .await;
    assert!(hidden["users"].as_array().unwrap().is_empty());
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/{}", b.uid))
                .bearer_auth(&a.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a disabled account has no single-get",
        )
        .await,
        "User account not found."
    );
    set_disable(pool, &b.uid, engine_api::db::time::EPOCH).await;
}
