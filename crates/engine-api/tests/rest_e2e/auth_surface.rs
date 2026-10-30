use super::*;

pub async fn auth_surface(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: auth providers");
    let key = server_key_header();
    let url = |route: &str, query: &str| format!("{base}/v2/account/authenticate{route}{query}");

    assert_eq!(
        refusal(
            client
                .post(url("/device", ""))
                .json(&json!({ "id": "e2e-no-key-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "authenticate without a server key",
        )
        .await,
        "Server key required"
    );
    assert_eq!(
        refusal(
            client
                .post(url("/device", ""))
                .header(
                    reqwest::header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD.encode("wrong-key:")
                    ),
                )
                .json(&json!({ "id": "e2e-wrong-key-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "authenticate with the wrong server key",
        )
        .await,
        "Server key invalid"
    );

    for (id, message) in [
        ("", "Device ID is required."),
        ("short", "Device ID invalid, must be 10-128 bytes."),
        (
            "has space in it",
            "Device ID invalid, no spaces or control characters allowed.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(url("/device", ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "id": id }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "device id validation",
            )
            .await,
            message
        );
    }
    let bad_bool = refusal(
        client
            .post(url("/device", "?create=maybe"))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": "e2e-bool-device" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "device create flag",
    )
    .await;
    assert!(
        bad_bool.contains("cannot parse \"maybe\" as a boolean value"),
        "{bad_bool}"
    );
    assert_eq!(
        refusal(
            client
                .post(url(
                    "/device",
                    &format!("?username={}", "bad[]name".replace(' ', "")),
                ))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": "e2e-bad-username-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "device username characters",
        )
        .await,
        "Username invalid, no spaces or control characters allowed."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?username=ab&create=true"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": "e2e-short-name-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "device username moderation",
        )
        .await,
        "Username must be 3-20 characters and free of profanity."
    );

    let device_id = format!("e2e-auth-device-{}", Uuid::new_v4());
    let username = clean_username("authplayer");
    let created = json_body(
        client
            .post(url("/device", &format!("?username={username}&create=true")))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a device account",
    )
    .await;
    assert_eq!(created["created"], true);
    let uid = jwt_uid(&created["token"]);
    let resumed = json_body(
        client
            .post(url("/device", "?create=false"))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "resume a device account",
    )
    .await;
    assert!(resumed.get("created").is_none());
    assert_eq!(jwt_uid(&resumed["token"]), uid);
    let stored: String = scalar(
        pool,
        "SELECT VALUE username FROM type::record('users', <uuid>$id) LIMIT 1",
        &[("id", json!(uid))],
    )
    .await
    .expect("the device account row");
    assert_eq!(stored, username);

    exec(
        pool,
        "UPDATE type::record('users', <uuid>$id) SET disable_time = $when RETURN NOTHING",
        &[
            ("id", json!(uid)),
            ("when", json!(engine_api::db::time::encode(Utc::now()))),
        ],
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "resume a banned account",
        )
        .await,
        "User account banned."
    );
    exec(
        pool,
        "UPDATE type::record('users', <uuid>$id) SET disable_time = $when RETURN NOTHING",
        &[
            ("id", json!(uid)),
            ("when", json!(engine_api::db::time::EPOCH)),
        ],
    )
    .await;
    let provider_device = format!("e2e-provider-device-{}", Uuid::new_v4());
    exec(
        pool,
        "CREATE type::record('user_device', $id) CONTENT $row RETURN NOTHING",
        &[
            ("id", json!(provider_device)),
            (
                "row",
                json!({
                    "user_id": uid,
                    "preferences": {},
                    "push_token_amazon": "",
                    "push_token_android": "",
                    "push_token_huawei": "",
                    "push_token_ios": "",
                    "push_token_web": "",
                    "provider": "steam",
                }),
            ),
        ],
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": provider_device }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "sign in through a provider-linked device row",
        )
        .await,
        "User account uses provider-linked identifier."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": format!("e2e-missing-device-{}", Uuid::new_v4()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "resume a device that was never created",
        )
        .await,
        "User account not found."
    );

    for (id, message) in [
        ("", "Custom ID is required."),
        ("short", "Custom ID invalid, must be 6-128 bytes."),
        (
            "has space",
            "Custom ID invalid, no spaces or control characters allowed.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(url("/custom", ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "id": id }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "custom id validation",
            )
            .await,
            message
        );
    }
    let custom_id = format!("custom-{}", letters_suffix());
    let custom_name = clean_username("custom");
    let custom_created = json_body(
        client
            .post(url(
                "/custom",
                &format!("?username={custom_name}&create=true"),
            ))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": custom_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a custom-id account",
    )
    .await;
    assert_eq!(custom_created["created"], true);
    let custom_uid = jwt_uid(&custom_created["token"]);
    let custom_resumed = json_body(
        client
            .post(url("/custom", ""))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": custom_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "resume a custom-id account",
    )
    .await;
    assert!(custom_resumed.get("created").is_none());
    assert_eq!(jwt_uid(&custom_resumed["token"]), custom_uid);
    assert_eq!(
        refusal(
            client
                .post(url(
                    "/custom",
                    &format!("?username={custom_name}&create=true"),
                ))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": format!("other-{}", letters_suffix()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "create a second account under a taken username",
        )
        .await,
        "Username is already in use."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/custom", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": format!("absent-{}", letters_suffix()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "custom id that was never created",
        )
        .await,
        "User account not found."
    );

    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email authenticate with no body",
        )
        .await,
        "Email address and password is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "nope", "password": "long-enough" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email address shape",
        )
        .await,
        "Invalid email address format."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "a@b.co", "password": "long-enough" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email address length",
        )
        .await,
        "Invalid email address, must be 10-255 bytes."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "e2e-short-password@example.com", "password": "short" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email password length",
        )
        .await,
        "Password must be at least 8 characters long."
    );
    let email_name = clean_username("mail");
    let email = format!("{email_name}@example.com");
    let password = "e2e-password";
    let email_created = json_body(
        client
            .post(url(
                "/email",
                &format!("?username={email_name}&create=true"),
            ))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "email": email, "password": password }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create an email account",
    )
    .await;
    assert_eq!(email_created["created"], true);
    let email_uid = jwt_uid(&email_created["token"]);
    let email_login = json_body(
        client
            .post(url("/email", ""))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "email": email.to_uppercase(), "password": password }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "log in by email (case-folded)",
    )
    .await;
    assert!(email_login.get("created").is_none());
    assert_eq!(jwt_uid(&email_login["token"]), email_uid);
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": email, "password": "wrong-password" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "email login with the wrong password",
        )
        .await,
        "Invalid credentials."
    );
    let username_login = json_body(
        client
            .post(url("/email", &format!("?username={email_name}")))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "email": "", "password": password }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "log in by username",
    )
    .await;
    assert_eq!(jwt_uid(&username_login["token"]), email_uid);
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "", "password": password }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "username login without a username",
        )
        .await,
        "Username is required when email address is not supplied."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": format!("absent{}@example.com", letters_suffix()), "password": password }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "email login that must not create",
        )
        .await,
        "User account not found."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", &format!("?username={email_name}&create=true")))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": format!("other{}@example.com", letters_suffix()), "password": password }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "create a second email account under a taken username",
        )
        .await,
        "Username is already in use."
    );

    for provider in ["apple", "steam"] {
        let label = if provider == "apple" {
            "Apple"
        } else {
            "Steam"
        };
        assert_eq!(
            refusal(
                client
                    .post(url(&format!("/{provider}"), ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "token": "vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "unconfigured vendor authenticate",
            )
            .await,
            format!("{label} authentication is not configured.")
        );
    }
    for provider in ["facebook", "google"] {
        let label = if provider == "facebook" {
            "Facebook"
        } else {
            "Google"
        };
        assert_eq!(
            refusal(
                client
                    .post(url(&format!("/{provider}"), ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({}))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "vendor token is required",
            )
            .await,
            format!("{label} access token is required.")
        );
        assert_eq!(
            refusal(
                client
                    .post(url(&format!("/{provider}"), ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "token": "vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                "vendor token cannot be verified",
            )
            .await,
            format!("Could not authenticate {label} profile.")
        );
    }
    assert_eq!(
        refusal(
            client
                .post(url("/facebookinstantgame", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "instant game signature is required",
        )
        .await,
        "Signed Player Info for a Facebook Instant Game is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/facebookinstantgame", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "signedPlayerInfo": "signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "instant game signature cannot be verified",
        )
        .await,
        "Error extracting the Facebook Instant Game player ID or validating the signature"
    );
    assert_eq!(
        refusal(
            client
                .post(url("/gamecenter", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "gamecenter credentials are required",
        )
        .await,
        "GameCenter access credentials are required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/gamecenter", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "bundle_id": "com.example.e2e" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "gamecenter fields are checked in order",
        )
        .await,
        "GameCenter player ID is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/gamecenter", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
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
            "gamecenter signature cannot be verified",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );

    assert_eq!(
        refusal(
            client
                .post(url("", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "runtime provider name is required",
        )
        .await,
        "Provider name is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "provider": "GitHub" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "unknown runtime provider",
        )
        .await,
        "Authentication provider not found: github"
    );
}
