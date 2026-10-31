use super::*;

pub async fn storage_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console storage browser");
    let owner = device_player(&env.client, &env.base, "store").await;
    let collection = format!("e2e_console_{}", letters_suffix());
    put_object(&env, &owner.token, &collection, "alpha", json!({"hp": 100})).await;
    put_object(&env, &owner.token, &collection, "beta", json!({"hp": 50})).await;

    let one = json_body(
        env.client
            .get(format!(
                "{}/v2/storage/{}/{}/{}",
                env.base, collection, "alpha", owner.uid
            ))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/storage/{collection}/{key}/{userId}",
    )
    .await;
    assert_eq!(json_string(&one["value"]), json!({"hp": 100}));

    let rows = json_body(
        env.client
            .get(format!("{}/v2/console/storage/{collection}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/storage/{collection}",
    )
    .await;
    let listed = rows.as_array().expect("an array of objects").clone();
    assert_eq!(listed.len(), 2, "{rows}");
    let alpha = listed
        .iter()
        .find(|row| row["key"] == "alpha")
        .expect("the alpha object is listed");
    assert_eq!(alpha["collection"], collection);
    assert_eq!(alpha["user_id"], owner.uid);
    assert_eq!(
        alpha["value"],
        json!({"hp": 100}),
        "the console parses the value back: {listed:?}"
    );

    let collections = json_body(
        env.client
            .get(format!("{}/v2/console/storage/collections", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/storage/collections",
    )
    .await;
    assert!(
        collections
            .as_array()
            .map(|names| names.iter().any(|name| name == &json!(collection)))
            .unwrap_or(false),
        "the new collection is named: {collections}"
    );

    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/storage/object/{collection}/alpha/not-a-uuid",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "deleting an object of an account that is not a uuid",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/storage/object/{collection}/gamma/{}",
                    env.base, owner.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting an object that does not exist",
        )
        .await,
        "Object not found."
    );
    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/storage/object/{collection}/alpha/{}",
                env.base, owner.uid
            ))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "deleting an object as a read-only operator",
    )
    .await;
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/storage/object/{collection}/alpha/{}",
                env.base, owner.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/storage/object/{collection}/{key}/{userId}",
    )
    .await;
    let rows = json_body(
        env.client
            .get(format!("{}/v2/console/storage/{collection}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the collection after the object delete",
    )
    .await;
    assert_eq!(rows.as_array().map(Vec::len), Some(1));

    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/storage/collection/{collection}",
                env.base
            ))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "clearing a collection as a read-only operator",
    )
    .await;
    let cleared = json_body(
        env.client
            .delete(format!(
                "{}/v2/console/storage/collection/{collection}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/storage/collection/{collection}",
    )
    .await;
    assert_eq!(cleared["removed"], 1, "{cleared}");
}
