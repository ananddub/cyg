use super::*;

pub async fn provider_auth_surface(env: &Harness) {
    eprintln!("gap: social authenticate verbs");
    let key = server_key_header();
    let url = |suffix: &str| format!("{}/v2/account/authenticate{suffix}", env.base);

    for (label, path) in [
        ("POST /v2/account/authenticate/apple", "/apple"),
        ("POST /v2/account/authenticate/facebook", "/facebook"),
        (
            "POST /v2/account/authenticate/facebookinstantgame",
            "/facebookinstantgame",
        ),
        ("POST /v2/account/authenticate/gamecenter", "/gamecenter"),
        ("POST /v2/account/authenticate/google", "/google"),
        ("POST /v2/account/authenticate/steam", "/steam"),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(url(path))
                    .json(&json!({ "token": "vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                &format!("{label} without the server key"),
            )
            .await,
            "Server key required",
            "{label}"
        );
    }
    assert_eq!(
        refusal(
            env.client
                .post(url("/apple"))
                .header(reqwest::header::AUTHORIZATION, "Basic bm90LW91ci1rZXk6")
                .json(&json!({ "token": "vendor-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "an authenticate with the wrong key",
        )
        .await,
        "Server key invalid"
    );

    assert_eq!(
        refusal(
            env.client
                .post(url("/apple"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "token": "e2e-gap-apple-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/apple",
        )
        .await,
        "Apple authentication is not configured."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/steam"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "token": "e2e-gap-steam-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/steam",
        )
        .await,
        "Steam authentication is not configured."
    );

    for (label, path, provider) in [
        (
            "POST /v2/account/authenticate/facebook",
            "/facebook",
            "Facebook",
        ),
        ("POST /v2/account/authenticate/google", "/google", "Google"),
    ] {
        assert_eq!(
            refusal(
                env.client
                    .post(url(path))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({}))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                &format!("{label} with an empty token"),
            )
            .await,
            format!("{provider} access token is required."),
            "{label}"
        );
        assert_eq!(
            refusal(
                env.client
                    .post(url(path))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "token": "e2e-gap-vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                label,
            )
            .await,
            format!("Could not authenticate {provider} profile."),
            "{label}"
        );
    }

    assert_eq!(
        refusal(
            env.client
                .post(url("/facebookinstantgame"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/facebookinstantgame with no signature",
        )
        .await,
        "Signed Player Info for a Facebook Instant Game is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/facebookinstantgame"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "signedPlayerInfo": "e2e-gap-signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "POST /v2/account/authenticate/facebookinstantgame",
        )
        .await,
        "Error extracting the Facebook Instant Game player ID or validating the signature"
    );

    assert_eq!(
        refusal(
            env.client
                .post(url("/gamecenter"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/account/authenticate/gamecenter with no body",
        )
        .await,
        "GameCenter access credentials are required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/gamecenter"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the bundle is the first field checked",
        )
        .await,
        "GameCenter bundle ID is required."
    );
    assert_eq!(
        refusal(
            env.client
                .post(url("/gamecenter"))
                .header(reqwest::header::AUTHORIZATION, &key)
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
            "POST /v2/account/authenticate/gamecenter",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );
}
