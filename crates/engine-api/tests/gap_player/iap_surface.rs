use super::*;

pub async fn iap_surface(env: &Harness) {
    eprintln!("gap: player iap purchases and subscriptions");
    let buyer = device_player(&env.client, &env.base, "buyer").await;
    let post = |path: &str| env.client.post(format!("{}{}", env.base, path)).bearer_auth(&buyer.token);

    assert_eq!(
        refusal(
            post("/v2/iap/purchase/apple")
                .json(&json!({ "receipt": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an apple purchase with no receipt",
        )
        .await,
        "receipt cannot be empty"
    );
    assert_eq!(
        refusal(
            post("/v2/iap/purchase/google")
                .json(&json!({ "purchase_token": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a google purchase with no token",
        )
        .await,
        "purchase token cannot be empty"
    );
    assert_eq!(
        refusal(
            post("/v2/iap/subscription/apple")
                .json(&json!({ "product_id": "season.none" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a subscription with neither credential",
        )
        .await,
        "receipt or purchase token is required"
    );

    let apple = json_body(
        post("/v2/iap/purchase/apple")
            .json(&json!({ "receipt": "e2e-gap-apple-receipt", "product_id": "gem.small" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/iap/purchase/apple",
    )
    .await;
    assert_eq!(apple["product_id"], "gem.small");
    assert_eq!(apple["transaction_id"].as_str().map(str::len), Some(32), "{apple}");
    assert_eq!(apple["environment"], "UNKNOWN", "nothing here can name the vendor environment");
    assert!(apple.get("refund_time").is_none(), "a fresh purchase has no refund");
    for (label, path, body) in [
        (
            "POST /v2/iap/purchase/google",
            "/v2/iap/purchase/google",
            json!({ "purchase_token": "e2e-gap-google-token", "product_id": "gem.large" }),
        ),
        (
            "POST /v2/iap/purchase/huawei",
            "/v2/iap/purchase/huawei",
            json!({ "purchase_token": "e2e-gap-huawei-token", "product_id": "gem.xl" }),
        ),
        (
            "POST /v2/iap/purchase/facebookinstant",
            "/v2/iap/purchase/facebookinstant",
            json!({ "signed_request": "e2e-gap-fi-token", "product_id": "gem.fb" }),
        ),
        (
            "POST /v2/iap/purchase/samsung",
            "/v2/iap/purchase/samsung",
            json!({ "purchase_id": "e2e-gap-samsung-id", "product_id": "gem.sam" }),
        ),
    ] {
        let stored = json_body(
            post(path).json(&body).send().await.unwrap(),
            StatusCode::OK,
            label,
        )
        .await;
        assert_eq!(
            stored["transaction_id"].as_str().map(str::len),
            Some(32),
            "{label} hashes its credential"
        );
    }

    for (label, path, body) in [
        (
            "POST /v2/iap/subscription/apple",
            "/v2/iap/subscription/apple",
            json!({
                "receipt": "e2e-gap-sub-receipt",
                "product_id": "season.gold",
                "expire_time": (Utc::now() + ChronoDuration::days(1)).to_rfc3339(),
            }),
        ),
        (
            "POST /v2/iap/subscription/google",
            "/v2/iap/subscription/google",
            json!({ "purchase_token": "e2e-gap-sub-token", "product_id": "season.silver" }),
        ),
        (
            "POST /v2/iap/subscription/huawei",
            "/v2/iap/subscription/huawei",
            json!({ "receipt": "e2e-gap-hw-sub", "product_id": "season.bronze" }),
        ),
    ] {
        let active = json_body(
            post(path).json(&body).send().await.unwrap(),
            StatusCode::OK,
            label,
        )
        .await;
        assert_eq!(active["state"], 1, "no expiry and no refund is an active {label}");
    }

    let purchases = json_body(
        env.client
            .get(format!("{}/v2/iap/purchase?limit=100", env.base))
            .bearer_auth(&buyer.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/iap/purchase",
    )
    .await;
    assert_eq!(purchases["purchases"].as_array().map(Vec::len), Some(5), "{purchases}");

    let subscriptions = json_body(
        env.client
            .get(format!("{}/v2/iap/subscription?limit=10", env.base))
            .bearer_auth(&buyer.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/iap/subscription",
    )
    .await;
    let held = subscriptions["subscriptions"]
        .as_array()
        .expect("an array of subscriptions")
        .clone();
    assert_eq!(held.len(), 1, "{subscriptions}");
    assert_eq!(held[0]["product_id"], "season.gold");
    assert!(
        held[0]["expiration_time"].as_i64().is_some(),
        "the listed subscription names its expiry: {held:?}"
    );

    let gold = json_body(
        env.client
            .get(format!("{}/v2/iap/subscription/season.gold", env.base))
            .bearer_auth(&buyer.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/iap/subscription/{productId}",
    )
    .await;
    assert_eq!(gold["product_id"], "season.gold");
    assert_eq!(gold["state"], 1);
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/iap/subscription/season.none", env.base))
                .bearer_auth(&buyer.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a product the buyer holds nothing for",
        )
        .await,
        "Subscription not found."
    );
}
