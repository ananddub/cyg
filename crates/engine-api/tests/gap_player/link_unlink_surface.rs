use super::*;

pub async fn link_unlink_surface(env: &Harness) {
    eprintln!("gap: social link and unlink verbs");
    let player = device_player(&env.client, &env.base, "linke").await;
    let post = |path: &str| {
        env.client
            .post(format!("{}/v2/account/link{path}", env.base))
            .bearer_auth(&player.token)
    };
    let unpost = |path: &str| {
        env.client
            .post(format!("{}/v2/account/unlink{path}", env.base))
            .bearer_auth(&player.token)
    };

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/account/link/google", env.base))
                .json(&json!({ "token": "vendor-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a link with no session",
        )
        .await,
        "Auth token required"
    );

    assert_eq!(
        refusal(
            post("/google").json(&json!({})).send().await.unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/google with no token",
        )
        .await,
        "Google access token is required."
    );
    assert_eq!(
        refusal(
            post("/google")
                .json(&json!({ "token": "e2e-gap-google-assertion" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/link/google",
        )
        .await,
        "Could not authenticate Google profile."
    );
    assert_eq!(
        refusal(
            post("/apple")
                .json(&json!({ "token": "e2e-gap-apple-assertion" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/apple",
        )
        .await,
        "Apple authentication is not configured."
    );
    assert_eq!(
        refusal(
            post("/gamecenter").json(&json!({})).send().await.unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/gamecenter missing its bundle",
        )
        .await,
        "GameCenter bundle ID is required."
    );
    assert_eq!(
        refusal(
            post("/gamecenter")
                .json(&json!({
                    "bundleId": "com.example.gap",
                    "playerId": "G:gap12345",
                    "timestampSeconds": 1_700_000_000i64,
                    "salt": "gap-salt",
                    "signature": "gap-signature",
                    "publicKeyUrl": "https://example.test/gap-key",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/link/gamecenter",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );
    assert_eq!(
        refusal(
            post("/facebookinstantgame")
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/link/facebookinstantgame with no signature",
        )
        .await,
        "Signed Player Info for a Facebook Instant Game is required."
    );
    assert_eq!(
        refusal(
            post("/facebookinstantgame")
                .json(&json!({ "signedPlayerInfo": "e2e-gap-signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/account/link/facebookinstantgame",
        )
        .await,
        "Could not authenticate Facebook Instant Game profile."
    );

    for (label, path, provider) in [
        ("unlink apple", "/apple", "Apple"),
        ("unlink facebook", "/facebook", "Facebook"),
        ("unlink google", "/google", "Google"),
    ] {
        assert_eq!(
            refusal(
                unpost(path)
                    .json(&json!({ "token": "e2e-gap-held-credential" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                &format!("{label} with a credential to refine"),
            )
            .await,
            format!("Could not authenticate {provider} profile."),
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            unpost("/facebookinstantgame")
                .json(&json!({ "signedPlayerInfo": "e2e-gap-signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "unlink facebookinstantgame with a signature to refine",
        )
        .await,
        "Could not authenticate Facebook Instant Game profile."
    );

    exec(
        env.db(),
        "UPDATE type::record('users', <uuid>$id) SET facebook_id = $fb RETURN NOTHING",
        &[
            ("id", json!(player.uid)),
            ("fb", json!(format!("e2e-gap-fb-{}", player.uid))),
        ],
    )
    .await;
    expect(
        unpost("/facebook").json(&json!({})).send().await.unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/facebook",
    )
    .await;
    let cleared: Option<Value> = scalar(
        env.db(),
        "SELECT VALUE facebook_id FROM type::record('users', <uuid>$id) LIMIT 1",
        &[("id", json!(player.uid))],
    )
    .await;
    assert!(
        matches!(cleared, None | Some(Value::Null)),
        "the empty credential cleared the column: {cleared:?}"
    );
    expect(
        unpost("/apple").json(&json!({})).send().await.unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/apple",
    )
    .await;
    expect(
        unpost("/google").json(&json!({})).send().await.unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/google",
    )
    .await;
    expect(
        unpost("/facebookinstantgame")
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/account/unlink/facebookinstantgame",
    )
    .await;

    let lone = device_player(&env.client, &env.base, "alone").await;
    exec(
        env.db(),
        "DELETE user_device WHERE (user_id = <uuid>$id OR user_id = $id) RETURN NOTHING",
        &[("id", json!(lone.uid))],
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/account/unlink/facebook", env.base))
                .bearer_auth(&lone.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlinking the only way in",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
}
