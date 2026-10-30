use super::*;

pub async fn iap_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: iap");
    let alice = device_player(client, base, "iap-a").await;
    let suffix = letters_suffix();
    let iap = format!("{base}/v2/iap");

    refusal(
        client
            .post(iap.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "receipt": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "validate an empty Apple receipt",
    )
    .await;

    let receipt = format!("e2e-receipt-{suffix}");
    let purchase = api_post(
        client,
        iap.clone(),
        &alice.token,
        json!({ "receipt": receipt, "product_id": "coin", "store": 0 }),
        "record a legacy Apple purchase",
    )
    .await;
    assert_eq!(purchase["product_id"], "coin");
    assert_eq!(purchase["transaction_id"], md5_hex(&receipt));
    assert_eq!(purchase["environment"], "UNKNOWN");
    assert!(purchase["purchase_time"].as_i64().expect("purchase time") > 0);
    assert!(purchase["update_time"].as_i64().expect("update time") > 0);
    assert!(purchase.get("refund_time").is_none());
    assert!(purchase.get("provider_notification").is_none());

    let replay = api_post(
        client,
        iap.clone(),
        &alice.token,
        json!({ "receipt": receipt, "product_id": "gem" }),
        "replay an Apple receipt",
    )
    .await;
    assert_eq!(replay["product_id"], "coin");
    assert_eq!(replay["transaction_id"], md5_hex(&receipt));

    let explicit = api_post(
        client,
        format!("{iap}/purchase/apple"),
        &alice.token,
        json!({ "receipt": format!("e2e-apple-{suffix}"), "product_id": "sword", "transaction_id": "e2e-tx-explicit" }),
        "record an Apple purchase with an explicit transaction id",
    )
    .await;
    assert_eq!(explicit["transaction_id"], "e2e-tx-explicit");
    assert_eq!(explicit["product_id"], "sword");

    refusal(
        client
            .post(format!("{iap}/purchase/google"))
            .bearer_auth(&alice.token)
            .json(&json!({ "purchase_token": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "record a Google purchase with no token",
    )
    .await;
    let google = api_post(
        client,
        format!("{iap}/purchase/google"),
        &alice.token,
        json!({ "purchase_token": format!("e2e-google-{suffix}"), "product_id": "gem" }),
        "record a Google purchase",
    )
    .await;
    assert_eq!(
        google["transaction_id"],
        md5_hex(&format!("e2e-google-{suffix}"))
    );
    assert_eq!(google["product_id"], "gem");

    let huawei = api_post(
        client,
        format!("{iap}/purchase/huawei"),
        &alice.token,
        json!({ "purchase_token": format!("e2e-huawei-{suffix}"), "product_id": "gem" }),
        "record a Huawei purchase",
    )
    .await;
    assert_eq!(
        huawei["transaction_id"],
        md5_hex(&format!("e2e-huawei-{suffix}"))
    );

    let facebook = api_post(
        client,
        format!("{iap}/purchase/facebookinstant"),
        &alice.token,
        json!({ "signed_request": format!("e2e-facebook-{suffix}"), "product_id": "gem" }),
        "record a Facebook Instant purchase through its alias",
    )
    .await;
    assert_eq!(
        facebook["transaction_id"],
        md5_hex(&format!("e2e-facebook-{suffix}"))
    );

    let samsung = api_post(
        client,
        format!("{iap}/purchase/samsung"),
        &alice.token,
        json!({ "purchase_id": format!("e2e-samsung-{suffix}"), "product_id": "gem" }),
        "record a Samsung purchase through its alias",
    )
    .await;
    assert_eq!(
        samsung["transaction_id"],
        md5_hex(&format!("e2e-samsung-{suffix}"))
    );

    let first = api_get(
        client,
        with_query(&format!("{iap}/purchase"), &[("limit", "3")]),
        &alice.token,
        "first purchase page",
    )
    .await;
    let mut seen: Vec<String> = first["purchases"]
        .as_array()
        .expect("purchases")
        .iter()
        .map(|row| {
            row["transaction_id"]
                .as_str()
                .expect("transaction")
                .to_owned()
        })
        .collect();
    assert_eq!(seen.len(), 3);
    let cursor = first["next_cursor"]
        .as_str()
        .expect("a purchase cursor")
        .to_owned();
    assert_eq!(
        cursor,
        base64::engine::general_purpose::STANDARD.encode("3")
    );
    let second = api_get(
        client,
        with_query(
            &format!("{iap}/purchase"),
            &[("limit", "3"), ("cursor", &cursor)],
        ),
        &alice.token,
        "second purchase page",
    )
    .await;
    seen.extend(
        second["purchases"]
            .as_array()
            .expect("purchases")
            .iter()
            .map(|row| {
                row["transaction_id"]
                    .as_str()
                    .expect("transaction")
                    .to_owned()
            }),
    );
    assert_eq!(seen.len(), 6);
    let cursor = second["next_cursor"]
        .as_str()
        .expect("the page was full")
        .to_owned();
    let third = api_get(
        client,
        with_query(
            &format!("{iap}/purchase"),
            &[("limit", "3"), ("cursor", &cursor)],
        ),
        &alice.token,
        "third purchase page",
    )
    .await;
    assert!(third["purchases"].as_array().expect("purchases").is_empty());
    assert_eq!(third["next_cursor"], "");

    let recovered = api_get(
        client,
        with_query(
            &format!("{iap}/purchase"),
            &[("limit", "3"), ("cursor", "garbage")],
        ),
        &alice.token,
        "purchase page with a malformed cursor",
    )
    .await;
    assert_eq!(
        recovered["purchases"].as_array().expect("purchases").len(),
        3
    );

    exec(
        pool,
        "UPDATE purchase SET refund_time = $when WHERE transaction_id = $tx RETURN NOTHING",
        &[
            ("when", json!(engine_api::db::time::encode(Utc::now()))),
            (
                "tx",
                json!(explicit["transaction_id"].as_str().expect("transaction")),
            ),
        ],
    )
    .await;
    let listed = api_get(
        client,
        with_query(&format!("{iap}/purchase"), &[("limit", "100")]),
        &alice.token,
        "purchases after a refund",
    )
    .await;
    assert_eq!(listed["purchases"].as_array().expect("purchases").len(), 5);

    refusal(
        client
            .post(format!("{iap}/subscription/apple"))
            .bearer_auth(&alice.token)
            .json(&json!({ "receipt": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "record a subscription with no credential",
    )
    .await;

    let apple_token = format!("e2e-sub-apple-{suffix}");
    let subscription = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({
            "receipt": apple_token,
            "product_id": "sub-a",
            "expire_time": "2030-01-01T00:00:00Z",
        }),
        "record an Apple subscription",
    )
    .await;
    assert_eq!(subscription["product_id"], "sub-a");
    assert_eq!(subscription["transaction_id"], md5_hex(&apple_token));
    assert_eq!(
        subscription["original_transaction_id"],
        md5_hex(&apple_token)
    );
    assert_eq!(subscription["expiration_time"], 1_893_456_000_i64);
    assert_eq!(subscription["state"], 1);
    assert_eq!(subscription["environment"], "UNKNOWN");
    assert!(subscription.get("refund_time").is_none());

    let google_token = format!("e2e-sub-google-{suffix}");
    let google_sub = api_post(
        client,
        format!("{iap}/subscription/google"),
        &alice.token,
        json!({
            "purchase_token": google_token,
            "product_id": "sub-b",
            "expire_time": "2031-01-01T00:00:00Z",
        }),
        "record a Google subscription",
    )
    .await;
    assert_eq!(google_sub["transaction_id"], md5_hex(&google_token));
    assert_eq!(google_sub["expiration_time"], 1_924_992_000_i64);

    let expired_token = format!("e2e-sub-expired-{suffix}");
    let expired = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({
            "receipt": expired_token,
            "product_id": "sub-expired",
            "expire_time": "2000-01-01T00:00:00Z",
        }),
        "record an expired subscription",
    )
    .await;
    assert_eq!(expired["state"], 3);
    assert_eq!(expired["expiration_time"], 946_684_800_i64);

    let open_ended = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({ "receipt": format!("e2e-sub-open-{suffix}"), "product_id": "sub-open" }),
        "record a subscription without an expiry",
    )
    .await;
    assert_eq!(open_ended["state"], 1);
    assert!(open_ended.get("expiration_time").is_none());

    let huawei_token = format!("e2e-sub-huawei-{suffix}");
    let huawei_sub = api_post(
        client,
        format!("{iap}/subscription/huawei"),
        &alice.token,
        json!({
            "purchase_token": huawei_token,
            "product_id": "sub-h",
            "expire_time": "2030-06-01T00:00:00Z",
        }),
        "record a Huawei subscription",
    )
    .await;
    assert_eq!(huawei_sub["transaction_id"], md5_hex(&huawei_token));

    let active = api_get(
        client,
        with_query(&format!("{iap}/subscription"), &[("limit", "100")]),
        &alice.token,
        "active subscriptions",
    )
    .await;
    let products: Vec<&str> = active["subscriptions"]
        .as_array()
        .expect("subscriptions")
        .iter()
        .map(|row| row["product_id"].as_str().expect("product"))
        .collect();
    assert_eq!(products.len(), 3);
    assert!(products.contains(&"sub-a"));
    assert!(products.contains(&"sub-b"));
    assert!(products.contains(&"sub-h"));
    assert!(!products.contains(&"sub-expired"));
    assert!(!products.contains(&"sub-open"));

    let by_product = api_get(
        client,
        format!("{iap}/subscription/sub-a"),
        &alice.token,
        "a subscription by product",
    )
    .await;
    assert_eq!(by_product["transaction_id"], md5_hex(&apple_token));
    refusal(
        client
            .get(format!("{iap}/subscription/sub-absent"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "an unknown subscription product",
    )
    .await;

    let renewed = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({
            "receipt": apple_token,
            "product_id": "sub-a2",
            "expire_time": "2032-01-01T00:00:00Z",
        }),
        "renew the Apple subscription",
    )
    .await;
    assert_eq!(renewed["transaction_id"], md5_hex(&apple_token));
    assert_eq!(renewed["product_id"], "sub-a2");
    assert_eq!(renewed["expiration_time"], 1_956_528_000_i64);
    refusal(
        client
            .get(format!("{iap}/subscription/sub-a"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "the replaced product is gone",
    )
    .await;
    let renewed = api_get(
        client,
        format!("{iap}/subscription/sub-a2"),
        &alice.token,
        "the renewed subscription",
    )
    .await;
    assert_eq!(renewed["transaction_id"], md5_hex(&apple_token));
    let still_active = api_get(
        client,
        with_query(&format!("{iap}/subscription"), &[("limit", "100")]),
        &alice.token,
        "subscriptions after a renewal",
    )
    .await;
    assert_eq!(
        still_active["subscriptions"]
            .as_array()
            .expect("subscriptions")
            .len(),
        3
    );

    let page = api_get(
        client,
        with_query(&format!("{iap}/subscription"), &[("limit", "2")]),
        &alice.token,
        "first subscription page",
    )
    .await;
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a subscription cursor")
        .to_owned();
    assert_eq!(
        cursor,
        base64::engine::general_purpose::STANDARD.encode("2")
    );
    let page = api_get(
        client,
        with_query(
            &format!("{iap}/subscription"),
            &[("limit", "2"), ("cursor", &cursor)],
        ),
        &alice.token,
        "second subscription page",
    )
    .await;
    assert_eq!(
        page["subscriptions"]
            .as_array()
            .expect("subscriptions")
            .len(),
        1
    );
}
