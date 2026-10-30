use super::*;

pub async fn storage_surface(client: &Client, base: &str) {
    eprintln!("e2e: storage");
    let alice = device_player(client, base, "st-a").await;
    let bob = device_player(client, base, "st-b").await;
    let collection = format!("e2e-storage-{}", letters_suffix());
    let storage = format!("{base}/v2/storage");

    let value1 = json!({ "n": 1 }).to_string();
    let value2 = json!({ "n": 2 }).to_string();
    let value3 = json!({ "n": 3 }).to_string();
    let version1 = md5_hex(&value1);
    let version2 = md5_hex(&value2);
    let version3 = md5_hex(&value3);

    let empty = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [] }),
        "write an empty batch",
    )
    .await;
    assert_eq!(empty, json!({}));

    for (body, what) in [
        (
            json!({ "objects": [{ "collection": "", "key": "a", "value": value1 }] }),
            "a write with no collection",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "", "value": value1 }] }),
            "a write with no key",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": "" }] }),
            "a write with no value",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": "[]" }] }),
            "a write with a non-object value",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": value1, "permission_read": 3 }] }),
            "a write with an out-of-range read permission",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": value1, "permission_write": 2 }] }),
            "a write with an out-of-range write permission",
        ),
    ] {
        refusal(
            client
                .put(storage.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }

    let ack = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": value1 }] }),
        "write a private object",
    )
    .await;
    let object = &ack["acks"][0];
    assert_eq!(object["collection"], collection);
    assert_eq!(object["key"], "a");
    assert_eq!(object["user_id"], alice.uid);
    assert_eq!(object["version"], version1);

    api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": value2, "permission_read": 2 }] }),
        "upsert a public object",
    )
    .await;

    refusal(
        client
            .put(storage.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "objects": [{ "collection": collection, "key": "a", "value": value3, "version": "deadbeef" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write with a stale version",
    )
    .await;
    let cas = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": value3, "version": version2, "permission_read": 2 }] }),
        "write with the current version",
    )
    .await;
    assert_eq!(cas["acks"][0]["version"], version3);

    let batch = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [
            { "collection": collection, "key": "sort-z", "value": json!({ "z": 1 }).to_string() },
            { "collection": collection, "key": "sort-a", "value": json!({ "a": 1 }).to_string() },
        ] }),
        "write an out-of-order batch",
    )
    .await;
    let keys: Vec<String> = batch["acks"]
        .as_array()
        .expect("acks")
        .iter()
        .map(|ack| ack["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["sort-z", "sort-a"]);

    let b_value = json!({ "b": 1 }).to_string();
    let c_value = json!({ "c": 1 }).to_string();
    api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [
            { "collection": collection, "key": "b", "value": b_value },
            { "collection": collection, "key": "c", "value": c_value, "permission_read": 2 },
            { "collection": collection, "key": "ro", "value": json!({ "ro": 1 }).to_string(), "permission_read": 2, "permission_write": 0 },
            { "collection": collection, "key": "star", "value": json!({ "star": 1 }).to_string(), "version": "*" },
        ] }),
        "write the remaining objects",
    )
    .await;
    refusal(
        client
            .put(storage.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "objects": [{ "collection": collection, "key": "star", "value": json!({ "other": 1 }).to_string(), "version": "*" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "insert-only write over an existing key",
    )
    .await;
    refusal(
        client
            .put(storage.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "objects": [{ "collection": collection, "key": "ro", "value": json!({ "ro": 2 }).to_string() }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "overwrite a read-only object",
    )
    .await;

    api_put(
        client,
        storage.clone(),
        &bob.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": json!({ "bob": 1 }).to_string(), "permission_read": 2 }] }),
        "bob writes his own object under a shared key",
    )
    .await;
    let unchanged = api_get(
        client,
        format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
        &alice.token,
        "alice's object survived bob's write",
    )
    .await;
    assert_eq!(json_string(&unchanged["value"]), json!({ "n": 3 }));
    assert_eq!(unchanged["user_id"], alice.uid);
    assert_eq!(unchanged["version"], version3);
    assert_eq!(unchanged["permission_read"], 2);
    assert_eq!(unchanged["permission_write"], 1);

    let read = api_post(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objectIds": [
            { "collection": collection, "key": "a" },
            { "collection": collection, "key": "b" },
        ] }),
        "read two own objects",
    )
    .await;
    let keys: Vec<String> = read["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["a", "b"]);
    assert_eq!(json_string(&read["objects"][0]["value"]), json!({ "n": 3 }));

    let bob_read = api_post(
        client,
        storage.clone(),
        &bob.token,
        json!({ "object_ids": [
            { "collection": collection, "key": "a" },
            { "collection": collection, "key": "b" },
        ] }),
        "read another account's objects",
    )
    .await;
    let keys: Vec<String> = bob_read["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(
        keys,
        vec!["a"],
        "an omitted owner means the caller; alice's private b stays hidden"
    );
    assert_eq!(
        json_string(&bob_read["objects"][0]["value"]),
        json!({ "bob": 1 }),
        "bob's own object under the shared key"
    );
    assert_eq!(bob_read["objects"][0]["user_id"], bob.uid);

    let empty_read = api_post(
        client,
        storage.clone(),
        &alice.token,
        json!({ "object_ids": [] }),
        "read an empty batch",
    )
    .await;
    assert_eq!(empty_read, json!({}));
    for (body, what) in [
        (
            json!({ "object_ids": [{ "collection": "", "key": "a" }] }),
            "read with no collection",
        ),
        (
            json!({ "object_ids": [{ "collection": collection, "key": "a", "user_id": "00000000-0000-0000-0000-000000000000" }] }),
            "read under the nil owner",
        ),
        (
            json!({ "object_ids": [{ "collection": collection, "key": "a", "user_id": "not-a-uuid" }] }),
            "read under a malformed owner",
        ),
    ] {
        refusal(
            client
                .post(storage.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }

    let one = api_get(
        client,
        format!("{base}/v2/storage/{collection}/a/00000000-0000-0000-0000-000000000000"),
        &alice.token,
        "read an own object by its full key",
    )
    .await;
    assert_eq!(json_string(&one["value"]), json!({ "n": 3 }));
    let versioned = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
            &[("version", &version3)],
        ),
        &alice.token,
        "read an object at its current version",
    )
    .await;
    assert_eq!(versioned["version"], version3);
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
                &[("version", "deadbeef")],
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read an object at a superseded version",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/b/{}", alice.uid))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read another account's private object",
    )
    .await;
    let public = api_get(
        client,
        format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
        &bob.token,
        "read another account's public object",
    )
    .await;
    assert_eq!(json_string(&public["value"]), json!({ "n": 3 }));
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/a/not-a-uuid"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read an object under a malformed owner",
    )
    .await;

    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}"),
            &[("limit", "2")],
        ),
        &bob.token,
        "first public listing page",
    )
    .await;
    let keys: Vec<String> = page["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["a", "a"]);
    let cursor = page["cursor"]
        .as_str()
        .expect("a listing cursor")
        .to_owned();
    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}"),
            &[("limit", "2"), ("cursor", &cursor)],
        ),
        &bob.token,
        "second public listing page",
    )
    .await;
    let keys: Vec<String> = page["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["c", "ro"]);
    assert!(page.get("cursor").is_none());
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/storage/{collection}"),
                &[("cursor", "garbage")],
            ))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "list storage with a malformed cursor",
    )
    .await;
    for limit in ["0", "101"] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/storage/{collection}"),
                    &[("limit", limit)],
                ))
                .bearer_auth(&bob.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list storage with a bad limit",
        )
        .await;
    }

    let own = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}/{}", alice.uid),
            &[("limit", "100")],
        ),
        &alice.token,
        "alice's own listing",
    )
    .await;
    let mut keys: Vec<String> = own["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["a", "b", "c", "ro", "sort-a", "sort-z", "star"]);
    let other = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}/{}", alice.uid),
            &[("limit", "100")],
        ),
        &bob.token,
        "another account's listing",
    )
    .await;
    let mut keys: Vec<String> = other["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["a", "c", "ro"]);
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/not-a-uuid"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "list under a malformed owner",
    )
    .await;

    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&alice.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "c", "version": "deadbeef" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete with a stale version",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&bob.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "b" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete another account's object",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&alice.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "missing" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a missing object",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&alice.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "ro" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a read-only object",
    )
    .await;
    let deleted = api_put(
        client,
        format!("{base}/v2/storage/delete"),
        &alice.token,
        json!({ "object_ids": [{ "collection": collection, "key": "c", "version": md5_hex(&c_value) }] }),
        "delete with the current version",
    )
    .await;
    assert_eq!(deleted, json!({}));
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/c/{}", alice.uid))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read a deleted object",
    )
    .await;
    let deleted = api_put(
        client,
        format!("{base}/v2/storage/delete"),
        &alice.token,
        json!({ "object_ids": [{ "collection": collection, "key": "a" }] }),
        "delete an object by key alone",
    )
    .await;
    assert_eq!(deleted, json!({}));
    let empty_delete = api_put(
        client,
        format!("{base}/v2/storage/delete"),
        &alice.token,
        json!({ "object_ids": [] }),
        "delete an empty batch",
    )
    .await;
    assert_eq!(empty_delete, json!({}));
}
