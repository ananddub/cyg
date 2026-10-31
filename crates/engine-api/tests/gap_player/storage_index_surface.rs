use super::*;

pub async fn storage_index_surface(env: &Harness) {
    eprintln!("gap: storage index and query");
    let suffix = letters_suffix();
    let owner = device_player(&env.client, &env.base, "sidx").await;
    let stranger = device_player(&env.client, &env.base, "sid2").await;
    let collection = format!("e2e_gap_idx_{suffix}");
    let index = format!("e2e_gap_index_{suffix}");

    for (key, hp) in [("obj-a", 100u32), ("obj-b", 200), ("obj-c", 300)] {
        put_object(env, &owner.token, &collection, key, json!({ "hp": hp })).await;
    }

    for (label, body, message) in [
        ("no name", json!({"collection": collection, "fields": ["hp"]}), "Storage index name must be set."),
        ("no collection", json!({"name": index, "fields": ["hp"]}), "Storage index collection must be set."),
        ("no fields", json!({"name": index, "collection": collection}), "Storage index fields must name at least one top-level key of the object value."),
        ("five fields", json!({"name": index, "collection": collection, "fields": ["a","b","c","d","e"]}), "Storage index fields must name at most four top-level keys."),
        ("zero max entries", json!({"name": index, "collection": collection, "fields": ["hp"], "maxEntries": 0}), "Storage index max entries must be greater than zero."),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(format!("{}/v2/storage/index", env.base))
                    .bearer_auth(&owner.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                &format!("an index definition with {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }
    let defined = json_body(
        env.client
            .post(format!("{}/v2/storage/index", env.base))
            .bearer_auth(&owner.token)
            .json(&json!({
                "name": index,
                "collection": collection,
                "key": "",
                "fields": ["hp"],
                "sortableFields": ["hp"],
                "maxEntries": 100,
                "indexOnly": false,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/storage/index",
    )
    .await;
    assert_eq!(defined["index"], index, "the definition names itself under `index`");
    assert_eq!(defined["collection"], collection);
    assert_eq!(defined["fields"], json!(["hp"]));
    assert_eq!(defined["max_entries"], 100);
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/storage/index", env.base))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": index, "collection": collection, "fields": ["hp"] }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "clashing with the one shared namespace",
        )
        .await,
        "Cannot create index: an index with this name already exists."
    );

    let listed = json_body(
        env.client
            .get(format!("{}/v2/storage/index", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/storage/index",
    )
    .await;
    assert!(
        listed["indexes"]
            .as_array()
            .map(|rows| rows.iter().any(|row| row["index"] == index))
            .unwrap_or(false),
        "the definition is listed: {listed}"
    );

    let url = |env: &Harness, name: &str| format!("{}/v2/storage/index/{name}/query", env.base);
    let matched = json_body(
        env.client
            .post(url(env, &index))
            .bearer_auth(&owner.token)
            .json(&json!({ "query": "hp:200", "limit": 10 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/storage/index/{indexId}/query (equality)",
    )
    .await;
    assert_eq!(matched["objects"].as_array().map(Vec::len), Some(1), "{matched}");
    assert_eq!(json_string(&matched["objects"][0]["value"]), json!({"hp": 200}));
    assert_eq!(matched["objects"][0]["key"], "obj-b");
    let ranged = json_body(
        env.client
            .post(url(env, &index))
            .bearer_auth(&owner.token)
            .json(&json!({ "query": "hp:[100 TO 300]", "limit": 10, "order": ["-hp"] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/storage/index/{indexId}/query (range, sorted)",
    )
    .await;
    let rows = ranged["objects"].as_array().expect("a page").clone();
    assert_eq!(rows.len(), 3, "{ranged}");
    assert_eq!(rows[0]["key"], "obj-c", "the '-' prefix sorts descending");

    assert_eq!(
        refusal(
            env.client
                .post(url(env, &index))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:100 OR hp:200", "limit": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an OR term the grammar does not speak",
        )
        .await,
        "Failed to parse the index query."
    );

    let mut keys: Vec<String> = Vec::new();
    let mut cursor = String::new();
    let mut first_cursor = String::new();
    for _ in 0..5 {
        let page = json_body(
            env.client
                .post(url(env, &index))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:[100 TO 300]", "limit": 1, "order": ["hp"], "cursor": cursor }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "POST /v2/storage/index/{indexId}/query (cursor paging)",
        )
        .await;
        for row in page["objects"].as_array().expect("a page").clone() {
            keys.push(row["key"].as_str().expect("key").to_owned());
        }
        cursor = page["next_cursor"].as_str().unwrap_or_default().to_owned();
        if first_cursor.is_empty() {
            first_cursor = cursor.clone();
        }
        if cursor.is_empty() {
            break;
        }
    }
    assert_eq!(keys, vec!["obj-a", "obj-b", "obj-c"], "the walk saw each object once");
    assert_eq!(
        refusal(
            env.client
                .post(url(env, &index))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:[100 TO 200]", "limit": 1, "order": ["hp"], "cursor": first_cursor }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "replaying a cursor with a drifted query",
        )
        .await,
        "Malformed cursor was used."
    );

    for (label, body, status, message) in [
        (
            "a field the index does not cover",
            json!({ "query": "mp:5", "limit": 10 }),
            StatusCode::BAD_REQUEST,
            "This index does not cover the field named in the query.",
        ),
        (
            "limit=0",
            json!({ "query": "hp:100", "limit": 0 }),
            StatusCode::BAD_REQUEST,
            "Invalid limit - limit must be between 1 and 100.",
        ),
        (
            "a term with no colon",
            json!({ "query": "justaword", "limit": 10 }),
            StatusCode::BAD_REQUEST,
            "Failed to parse the index query.",
        ),
        (
            "an order field that is neither a column nor sortable",
            json!({ "query": "hp:100", "limit": 10, "order": ["nope"] }),
            StatusCode::BAD_REQUEST,
            "This index cannot sort results by the field named in the order.",
        ),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(url(env, &index))
                    .bearer_auth(&owner.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                status,
                &format!("the query refusing {label}"),
            )
            .await,
            message,
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/storage/index/e2e_missing_{suffix}/query", env.base))
                .bearer_auth(&owner.token)
                .json(&json!({ "query": "hp:100", "limit": 10 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "querying an index that was never registered",
        )
        .await,
        "Index not found."
    );

    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/storage/index/{index}", env.base))
                .bearer_auth(&stranger.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a stranger trying to drop the definition",
        )
        .await,
        "Only the player who created this index can delete it."
    );
    expect(
        env.client
            .delete(format!("{}/v2/storage/index/{index}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/storage/index/{indexId}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/storage/index/{index}", env.base))
                .bearer_auth(&owner.token)
                .send()
                .await
            .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting the same definition twice",
        )
        .await,
        "Index not found."
    );
}
