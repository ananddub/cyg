use super::*;

pub async fn storage_index_surface(client: &Client, base: &str) {
    eprintln!("e2e: storage indexes");
    let alice = device_player(client, base, "si-a").await;
    let bob = device_player(client, base, "si-b").await;
    let suffix = letters_suffix();
    let collection = format!("e2e-index-{suffix}");
    let index_name = format!("e2e-idx-{suffix}");
    let index_only_name = format!("e2e-idx-only-{suffix}");
    let window_name = format!("e2e-idx-window-{suffix}");
    let index_url = format!("{base}/v2/storage/index");

    let objects = [
        ("k1", json!({ "level": 12, "name": "sword", "score": 30 })),
        ("k2", json!({ "level": 18, "name": "shield", "score": 10 })),
        ("k3", json!({ "level": 25, "name": "bow", "score": 20 })),
    ];
    let writes: Vec<Value> = objects
        .iter()
        .map(|(key, value)| {
            json!({
                "collection": collection,
                "key": key,
                "value": value.to_string(),
            })
        })
        .collect();
    api_put(
        client,
        format!("{base}/v2/storage"),
        &alice.token,
        json!({ "objects": writes }),
        "seed indexed objects",
    )
    .await;

    for (body, what) in [
        (
            json!({ "collection": collection, "fields": ["level"] }),
            "an index with no name",
        ),
        (
            json!({ "name": index_name, "fields": ["level"] }),
            "an index with no collection",
        ),
        (
            json!({ "name": index_name, "collection": collection, "fields": [] }),
            "an index with no fields",
        ),
        (
            json!({
                "name": index_name,
                "collection": collection,
                "fields": ["a", "b", "c", "d", "e"],
            }),
            "an index with too many fields",
        ),
        (
            json!({
                "name": index_name,
                "collection": collection,
                "fields": ["level"],
                "max_entries": 0,
            }),
            "an index with a non-positive window",
        ),
    ] {
        refusal(
            client
                .post(index_url.clone())
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

    let created = api_post(
        client,
        index_url.clone(),
        &alice.token,
        json!({
            "name": index_name,
            "collection": collection,
            "fields": ["level", "name", "score"],
            "sortable_fields": ["score"],
        }),
        "create an index",
    )
    .await;
    assert_eq!(created["index"], index_name);
    assert_eq!(created["collection"], collection);
    assert_eq!(created["max_entries"], 10000);
    assert_eq!(created["fields"], json!(["level", "name", "score"]));
    assert_eq!(created["sortable_fields"], json!(["score"]));
    assert!(created.get("index_only").is_none());

    refusal(
        client
            .post(index_url.clone())
            .bearer_auth(&alice.token)
            .json(&json!({
                "name": index_name,
                "collection": collection,
                "fields": ["level"],
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "create a clashing index name",
    )
    .await;

    let listed = api_get(client, index_url.clone(), &alice.token, "list indexes").await;
    let names: Vec<String> = listed["indexes"]
        .as_array()
        .expect("indexes")
        .iter()
        .map(|index| index["index"].as_str().expect("index name").to_owned())
        .collect();
    assert!(names.contains(&index_name));

    let query_url = format!("{base}/v2/storage/index/{index_name}/query");
    for (body, status, what) in [
        (
            json!({ "query": "badterm" }),
            StatusCode::BAD_REQUEST,
            "parse a malformed index query",
        ),
        (
            json!({ "query": "missing:1" }),
            StatusCode::BAD_REQUEST,
            "query a field the index does not cover",
        ),
        (
            json!({ "query": "", "limit": 0 }),
            StatusCode::BAD_REQUEST,
            "query an index with a bad limit",
        ),
        (
            json!({ "query": "", "order": ["level"] }),
            StatusCode::BAD_REQUEST,
            "order by a field the index cannot sort",
        ),
    ] {
        refusal(
            client
                .post(query_url.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            status,
            what,
        )
        .await;
    }

    let matched = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "level:12" }),
        "query an equality term",
    )
    .await;
    let keys: Vec<String> = matched["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["k1"]);
    assert_eq!(
        json_string(&matched["objects"][0]["value"])["name"],
        "sword"
    );

    let ranged = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "score:[15 TO 30]" }),
        "query a numeric range",
    )
    .await;
    let mut keys: Vec<String> = ranged["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["k1", "k3"]);

    let sorted = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "order": ["-score"] }),
        "query ordered by a descending field",
    )
    .await;
    let keys: Vec<String> = sorted["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["k1", "k3", "k2"]);
    let prefixed = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "order": ["-value.score"] }),
        "query ordered through the value prefix",
    )
    .await;
    assert_eq!(prefixed["objects"], sorted["objects"]);

    let first_page = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "limit": 2 }),
        "first indexed page",
    )
    .await;
    assert_eq!(first_page["objects"].as_array().expect("objects").len(), 2);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("an index cursor")
        .to_owned();
    let second_page = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "limit": 2, "cursor": cursor }),
        "second indexed page",
    )
    .await;
    let keys: Vec<String> = second_page["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["k3"]);
    assert!(second_page.get("next_cursor").is_none());
    refusal(
        client
            .post(query_url.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "query": "level:12", "limit": 2, "cursor": cursor }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "replay an index cursor against another query",
    )
    .await;

    let hidden = api_post(
        client,
        query_url.clone(),
        &bob.token,
        json!({ "query": "" }),
        "query an index as another account",
    )
    .await;
    assert_eq!(hidden, json!({}));

    api_post(
        client,
        index_url.clone(),
        &alice.token,
        json!({
            "name": index_only_name,
            "collection": collection,
            "fields": ["level"],
            "index_only": true,
        }),
        "create an index-only definition",
    )
    .await;
    let projected = api_post(
        client,
        format!("{base}/v2/storage/index/{index_only_name}/query"),
        &alice.token,
        json!({ "query": "" }),
        "read an index-only projection",
    )
    .await;
    for object in projected["objects"].as_array().expect("objects") {
        let value = json_string(&object["value"]);
        assert_eq!(value.as_object().expect("object").len(), 1);
        assert!(value.get("level").is_some());
    }

    api_post(
        client,
        index_url.clone(),
        &alice.token,
        json!({
            "name": window_name,
            "collection": collection,
            "fields": ["level"],
            "max_entries": 2,
        }),
        "create a two-entry index",
    )
    .await;
    let capped = api_post(
        client,
        format!("{base}/v2/storage/index/{window_name}/query"),
        &alice.token,
        json!({ "query": "" }),
        "read a capped index",
    )
    .await;
    assert_eq!(capped["objects"].as_array().expect("objects").len(), 2);
    let cursor = capped["next_cursor"]
        .as_str()
        .expect("a capped index cursor")
        .to_owned();
    let exhausted = api_post(
        client,
        format!("{base}/v2/storage/index/{window_name}/query"),
        &alice.token,
        json!({ "query": "", "cursor": cursor }),
        "read past the capped window",
    )
    .await;
    assert_eq!(exhausted, json!({}));

    refusal(
        client
            .delete(format!("{base}/v2/storage/index/{index_name}"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "delete another account's index",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/storage/index/absent-{suffix}"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete an unknown index",
    )
    .await;
    for name in [&index_name, &index_only_name, &window_name] {
        api_delete(
            client,
            format!("{base}/v2/storage/index/{name}"),
            &alice.token,
            "delete an index",
        )
        .await;
    }
    refusal(
        client
            .post(query_url)
            .bearer_auth(&alice.token)
            .json(&json!({ "query": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "query a deleted index",
    )
    .await;
    let listed = api_get(client, index_url, &alice.token, "indexes after deletion").await;
    let names: Vec<String> = listed["indexes"]
        .as_array()
        .map(|indexes| {
            indexes
                .iter()
                .map(|index| index["index"].as_str().expect("index name").to_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(!names.contains(&index_name));
}
