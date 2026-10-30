use super::*;

pub async fn purchase_idempotency(env: &Harness) {
    eprintln!("gap: idempotent purchase recording");
    let player = device_player(&env.client, &env.base, "player2").await;

    let first = json_body(
        env.client
            .post(format!("{}/v2/iap/purchase/apple", env.base))
            .bearer_auth(&player.token)
            .header("Idempotency-Key", "buy-1")
            .json(&json!({"receipt": "receipt-abcdef"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "recording an apple purchase",
    )
    .await;
    assert!(
        first["transaction_id"].is_string(),
        "the receipt verifies (unverified in tests) and records: {first}"
    );

    let replay = json_body(
        env.client
            .post(format!("{}/v2/iap/purchase/apple", env.base))
            .bearer_auth(&player.token)
            .header("Idempotency-Key", "buy-1")
            .json(&json!({"receipt": "receipt-abcdef"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "replaying the purchase request",
    )
    .await;
    assert_eq!(
        replay["transaction_id"], first["transaction_id"],
        "the replay returns the stored response: {replay}"
    );

    let rows = rows_matching(env, "purchase", &player.uid).await;
    assert_eq!(rows.len(), 1, "one purchase row after the replay: {rows:?}");

    let text = refusal(
        env.client
            .post(format!("{}/v2/iap/purchase/apple", env.base))
            .bearer_auth(&player.token)
            .header("Idempotency-Key", "buy-1")
            .json(&json!({"receipt": "a-different-receipt"}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "reusing the key for a different receipt",
    )
    .await;
    assert!(
        text.contains("already used for a different request"),
        "the reuse refusal names the collision: {text}"
    );
}
