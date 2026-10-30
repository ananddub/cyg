use super::*;

pub async fn grant_idempotency(env: &Harness, console: &str) {
    eprintln!("gap: idempotent, stackable inventory grants");
    let player = device_player(&env.client, &env.base, "grant").await;
    let item_id = format!("gem-idem-{}", uid());

    json_body(
        env.client
            .post(format!("{}/v2/inventory/define", env.base))
            .bearer_auth(console)
            .json(&json!({
                "item_id": item_id,
                "name": "Idempotency Gem",
                "category": "gem",
                "max_stack": 5,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "defining a stackable gem",
    )
    .await;

    let grant = |key: &str, quantity: i32| {
        let client = env.client.clone();
        let base = env.base.clone();
        let uid = player.uid.clone();
        let item = item_id.clone();
        let key = key.to_owned();
        async move {
            client
                .post(format!("{base}/v2/inventory/grant"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .header("Idempotency-Key", key)
                .json(&json!({"user_id": uid, "item_id": item, "quantity": quantity}))
                .send()
                .await
                .unwrap()
        }
    };

    let first = json_body(grant("grant-1", 2).await, StatusCode::OK, "grant 2 gems").await;
    assert_eq!(
        first["item"]["quantity"],
        json!(2),
        "first grant creates the stack: {first}"
    );

    let replay = json_body(
        grant("grant-1", 2).await,
        StatusCode::OK,
        "replay the grant",
    )
    .await;
    assert_eq!(
        replay["item"]["instance_id"], first["item"]["instance_id"],
        "the replay returns the stored response: {replay}"
    );

    let rows = rows_matching(env, "player_inventory_item", &player.uid).await;
    assert_eq!(rows.len(), 1, "the replay granted nothing twice: {rows:?}");

    let merged = json_body(
        grant("grant-2", 3).await,
        StatusCode::OK,
        "merge into the stack",
    )
    .await;
    assert_eq!(
        merged["item"]["quantity"],
        json!(5),
        "a second grant stacks onto the open instance: {merged}"
    );

    let overflow = json_body(
        grant("grant-3", 2).await,
        StatusCode::OK,
        "overflow creates a row",
    )
    .await;
    assert_eq!(
        overflow["item"]["quantity"],
        json!(2),
        "a full stack forces a new instance: {overflow}"
    );

    let rows = rows_matching(env, "player_inventory_item", &player.uid).await;
    assert_eq!(rows.len(), 2, "two instances hold seven gems: {rows:?}");

    let text = refusal(
        grant("grant-1", 5).await,
        StatusCode::BAD_REQUEST,
        "reusing the grant key for a different quantity",
    )
    .await;
    assert!(
        text.contains("already used for a different request"),
        "the reuse refusal names the collision: {text}"
    );
}
