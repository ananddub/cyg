use super::*;

pub async fn link_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: link and unlink");
    let a = device_player(client, base, "linka").await;
    let b = device_player(client, base, "linkb").await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/device"))
                .json(&json!({ "id": "e2e-anonymous-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "link without a token",
        )
        .await,
        "Auth token required"
    );
    for (route, body, message) in [
        (
            "device",
            json!({}),
            "Device ID is required.",
        ),
        (
            "device",
            json!({ "id": "short" }),
            "Device ID invalid, must be 10-128 bytes.",
        ),
        (
            "custom",
            json!({}),
            "Custom ID is required.",
        ),
        (
            "custom",
            json!({ "id": "short" }),
            "Invalid custom ID, must be 6-128 bytes.",
        ),
        (
            "custom",
            json!({ "id": "has space" }),
            "Invalid custom ID, no spaces or control characters allowed.",
        ),
        (
            "email",
            json!({}),
            "Email address and password is required.",
        ),
        (
            "email",
            json!({ "email": "e2e-link@example.com", "password": "short" }),
            "Password must be at least 8 characters long.",
        ),
        (
            "email",
            json!({ "email": "nope", "password": "long-enough" }),
            "Invalid email address format.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(format!("{base}/v2/account/link/{route}"))
                    .bearer_auth(&a.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "link validation",
            )
            .await,
            message
        );
    }

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": b.device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink B's last identifier",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );

    let custom = format!("link-{}", letters_suffix());
    json_body(
        client
            .post(format!("{base}/v2/account/link/custom"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link a custom id",
    )
    .await;
    let b_custom = format!("blink-{}", letters_suffix());
    json_body(
        client
            .post(format!("{base}/v2/account/link/custom"))
            .bearer_auth(&b.token)
            .json(&json!({ "id": b_custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link B's custom id",
    )
    .await;
    let second_device = format!("e2e-second-device-{}", Uuid::new_v4());
    json_body(
        client
            .post(format!("{base}/v2/account/link/device"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": second_device }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link a second device",
    )
    .await;
    let email = format!("link{}@example.com", letters_suffix());
    json_body(
        client
            .post(format!("{base}/v2/account/link/email"))
            .bearer_auth(&a.token)
            .json(&json!({ "email": email, "password": "e2e-password" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link an email",
    )
    .await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/custom"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": custom }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim a custom id already linked",
        )
        .await,
        "Custom ID is already in use."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/device"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": second_device }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim a device already linked",
        )
        .await,
        "Device ID already in use."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/email"))
                .bearer_auth(&b.token)
                .json(&json!({ "email": email, "password": "e2e-password" }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim an email already linked",
        )
        .await,
        "Email is already in use."
    );

    let account = json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the linked account",
    )
    .await;
    assert_eq!(account["custom_id"], custom);
    assert_eq!(account["email"], email.to_lowercase());
    let devices = account["device_accounts"].as_array().unwrap();
    assert_eq!(devices.len(), 2);
    assert!(devices.iter().any(|device| device["id"] == second_device));

    for (route, body, status, message) in [
        (
            "apple",
            json!({ "token": "vendor-token" }),
            StatusCode::BAD_REQUEST,
            "Apple authentication is not configured.",
        ),
        (
            "steam",
            json!({}),
            StatusCode::BAD_REQUEST,
            "Steam access token is required.",
        ),
        (
            "steam",
            json!({ "account": { "token": "vendor-token" } }),
            StatusCode::BAD_REQUEST,
            "Steam authentication is not configured.",
        ),
        (
            "facebook",
            json!({}),
            StatusCode::BAD_REQUEST,
            "Facebook access token is required.",
        ),
        (
            "facebook",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook profile.",
        ),
        (
            "google",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Google profile.",
        ),
        (
            "facebookinstantgame",
            json!({}),
            StatusCode::BAD_REQUEST,
            "Signed Player Info for a Facebook Instant Game is required.",
        ),
        (
            "facebookinstantgame",
            json!({ "signed_player_info": "signed" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook Instant Game profile.",
        ),
        (
            "gamecenter",
            json!({ "bundle_id": "com.example.e2e" }),
            StatusCode::BAD_REQUEST,
            "GameCenter player ID is required.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(format!("{base}/v2/account/link/{route}"))
                    .bearer_auth(&a.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                status,
                "vendor link",
            )
            .await,
            message
        );
    }
    let bad_sync = refusal(
        client
            .post(format!("{base}/v2/account/link/facebook?sync=maybe"))
            .bearer_auth(&a.token)
            .json(&json!({ "token": "vendor-token" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "facebook sync flag",
    )
    .await;
    assert!(bad_sync.contains("cannot parse \"maybe\" as a boolean value"));
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link"))
                .bearer_auth(&a.token)
                .json(&json!({ "provider": "GitHub" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "runtime provider link",
        )
        .await,
        "Authentication provider not found: github"
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&a.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "unlink without a device id",
        )
        .await,
        "A device ID must be supplied."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&a.token)
                .json(&json!({ "id": format!("e2e-not-owned-{}", Uuid::new_v4()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink a device the account does not own",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/custom"))
                .bearer_auth(&a.token)
                .json(&json!({ "id": format!("wrong-{}", letters_suffix()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink with the wrong custom id",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/email"))
                .bearer_auth(&a.token)
                .json(&json!({ "email": "wrong@example.com" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink with the wrong email",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );

    json_body(
        client
            .post(format!("{base}/v2/account/unlink/custom"))
            .bearer_auth(&b.token)
            .json(&json!({ "id": b_custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink B's custom id",
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": b.device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "B's device is the last identifier again",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/custom"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink A's custom id",
    )
    .await;
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/email"))
            .bearer_auth(&a.token)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink A's email",
    )
    .await;
    let unlinked = json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the unlinked account",
    )
    .await;
    assert!(unlinked["custom_id"].is_null());
    assert!(unlinked["email"].is_null());
    let stored_password = scalar::<Value>(
        pool,
        "SELECT VALUE password FROM type::record('users', <uuid>$id) LIMIT 1",
        &[("id", json!(a.uid))],
    )
    .await;
    assert!(
        matches!(stored_password, None | Some(Value::Null)),
        "unlinking an email drops the password"
    );

    for (route, body, status, message) in [
        (
            "apple",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Apple profile.",
        ),
        (
            "facebook",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook profile.",
        ),
        (
            "google",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Google profile.",
        ),
        (
            "steam",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Steam profile.",
        ),
        (
            "facebookinstantgame",
            json!({ "signed_player_info": "signed" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook Instant Game profile.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(format!("{base}/v2/account/unlink/{route}"))
                    .bearer_auth(&a.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                status,
                "vendor unlink with a credential",
            )
            .await,
            message
        );
        json_body(
            client
                .post(format!("{base}/v2/account/unlink/{route}"))
                .bearer_auth(&a.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "vendor unlink without a credential",
        )
        .await;
    }
    let partial_gamecenter = json!({ "bundle_id": "com.example.e2e" });
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/gamecenter"))
            .bearer_auth(&a.token)
            .json(&partial_gamecenter)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a partial gamecenter bundle is not a refinement",
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/gamecenter"))
                .bearer_auth(&a.token)
                .json(&json!({
                    "bundle_id": "com.example.e2e",
                    "player_id": "G:12345",
                    "timestamp_seconds": 1_700_000_000i64,
                    "salt": "salt",
                    "signature": "signature",
                    "public_key_url": "https://example.test/key",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a complete gamecenter bundle must be verified",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink"))
                .bearer_auth(&a.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "unlink without a provider name",
        )
        .await,
        "A provider name must be supplied."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink"))
                .bearer_auth(&a.token)
                .json(&json!({ "provider": "steam" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink a provider with no rows",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );

    json_body(
        client
            .post(format!("{base}/v2/account/unlink/device"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": second_device }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink the second device",
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&a.token)
                .json(&json!({ "id": a.device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "the last device cannot go",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
}
