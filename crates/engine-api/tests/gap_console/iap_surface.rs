use super::*;

pub async fn iap_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console iap validate, ledger and reset");
    let player = device_player(&env.client, &env.base, "iap").await;

    let dry = json_body(
        env.client
            .post(format!("{}/v2/console/iap/apple/validate", env.base))
            .bearer_auth(console)
            .json(&json!({ "receipt": "e2e-apple-receipt", "product_id": "gem.100", "persist": false }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "an unpersisted purchase validation",
    )
    .await;
    let parsed = &dry["validated_purchases"][0];
    assert_eq!(parsed["product_id"], "gem.100");
    assert_eq!(
        parsed["transaction_id"].as_str().map(str::len),
        Some(32),
        "the credential is hashed into the transaction id: {dry}"
    );
    assert!(
        dry["validated_purchases"][0]
            .get("provider_notification")
            .is_none(),
        "a plain purchase reply carries no notification"
    );
    assert_eq!(
        json_body(
            env.client
                .get(format!("{}/v2/console/purchase", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "the ledger stays empty after a dry validation",
        )
        .await
        .as_array()
        .map(Vec::len)
        .unwrap_or_default(),
        0
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/apple/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "e2e-apple-receipt" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "persisting with no user id",
        )
        .await,
        "Persisting a validation needs a user id."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/apple/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "x", "user_id": "not-a-uuid" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "persisting for an account that is not a uuid",
        )
        .await,
        "Invalid account id."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/apple/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "user_id": player.uid, "kind": "renewal" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an unknown validation kind",
        )
        .await,
        "Validation kind must be \"purchase\" or \"subscription\"."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/nvidia/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "x" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an unknown vendor",
        )
        .await,
        "Unknown IAP vendor: nvidia"
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/samsung/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "x", "kind": "subscription" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a subscription for a vendor that has none",
        )
        .await,
        "Samsung subscriptions are not supported."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/google/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "user_id": player.uid }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a validation with no credential",
        )
        .await,
        "receipt or purchase token is required"
    );
    refusal(
        env.client
            .post(format!("{}/v2/console/iap/google/validate", env.base))
            .bearer_auth(viewer)
            .json(&json!({ "receipt": "x", "persist": false }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "validating as a read-only operator",
    )
    .await;

    let stored = json_body(
        env.client
            .post(format!("{}/v2/console/iap/google/validate", env.base))
            .bearer_auth(console)
            .json(&json!({
                "purchase_token": "e2e-google-token",
                "product_id": "gems.large",
                "user_id": player.uid,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/iap/{vendor}/validate",
    )
    .await;
    assert_eq!(stored["validated_purchases"][0]["product_id"], "gems.large");

    let subscriptions = json_body(
        env.client
            .post(format!("{}/v2/console/iap/google/validate", env.base))
            .bearer_auth(console)
            .json(&json!({
                "purchase_token": "e2e-google-sub-token",
                "product_id": "season.pass",
                "user_id": player.uid,
                "kind": "subscription",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "validating a subscription",
    )
    .await;
    assert_eq!(subscriptions["subscription"]["product_id"], "season.pass");

    let purchases = json_body(
        env.client
            .get(format!("{}/v2/console/purchase?limit=10", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/purchase",
    )
    .await;
    assert_eq!(
        purchases
            .as_array()
            .map(|rows| rows.iter().any(|row| row["product_id"] == "gems.large"))
            .unwrap_or(false),
        true,
        "the validated purchase is in the ledger: {purchases}"
    );
    let subs = json_body(
        env.client
            .get(format!("{}/v2/console/subscription", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/subscription",
    )
    .await;
    let rows = subs.as_array().expect("an array of subscriptions").clone();
    assert_eq!(rows.len(), 1, "{subs}");
    assert_eq!(rows[0]["product_id"], "season.pass");
    assert_eq!(rows[0]["user_id"], player.uid);

    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/subscription/%20", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "deleting every subscription of an unnamed product",
        )
        .await,
        "A product id is required."
    );
    expect(
        env.client
            .delete(format!("{}/v2/console/subscription/season.pass", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/subscription/{product_id}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/subscription/season.pass", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting the same product twice",
        )
        .await,
        "Subscription not found for this product."
    );

    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/iap/google/reset", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "resetting a vendor that was never configured",
        )
        .await,
        "No stored Google IAP configuration."
    );
    exec(
        env.db(),
        "CREATE type::record('setting', $name) CONTENT \
         { name: $name, value: $value, update_time: $update_time } RETURN NOTHING",
        &[
            ("name", json!("iap.google")),
            ("value", json!({"token": "old"})),
            (
                "update_time",
                json!(engine_api::db::time::encode(chrono::Utc::now())),
            ),
        ],
    )
    .await;
    expect(
        env.client
            .delete(format!("{}/v2/console/iap/google/reset", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/iap/{vendor}/reset",
    )
    .await;
    let left = count(
        env.db(),
        "setting",
        Some("name = $name"),
        &[("name", json!("iap.google"))],
    )
    .await;
    assert_eq!(left, 0, "the reset clears the stored configuration");
}
