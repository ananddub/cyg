use super::*;

pub async fn report_surface(env: &Harness, console: &str) {
    eprintln!("gap: reports, moderation gate and mute enforcement");
    let rep = device_player(&env.client, &env.base, "rp").await;
    let tgt = device_player(&env.client, &env.base, "rgt").await;
    let reports = |suffix: &str| format!("{}/v2/reports{suffix}", env.base);

    assert_eq!(
        refusal(
            env.client
                .post(reports(""))
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "POST /v2/reports without a session",
        )
        .await,
        "Auth token required"
    );

    assert_eq!(
        refusal(
            env.client
                .post(reports(""))
                .bearer_auth(&rep.token)
                .json(&json!({
                    "reported_id": tgt.uid,
                    "reason": "being-boring",
                    "details": "no reason at all",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/reports with an unknown reason",
        )
        .await,
        "Invalid report reason: must be harassment, cheating, exploit, inappropriate_content or other."
    );

    assert_eq!(
        refusal(
            env.client
                .post(reports(""))
                .bearer_auth(&rep.token)
                .json(&json!({
                    "reported_id": rep.uid,
                    "reason": "harassment",
                    "details": "myself",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "POST /v2/reports against yourself",
        )
        .await,
        "Cannot report your own account."
    );

    let filed = json_body(
        env.client
            .post(reports(""))
            .bearer_auth(&rep.token)
            .json(&json!({
                "reported_id": tgt.uid,
                "reason": "harassment",
                "details": "fuck you and your shitty account",
                "path": "2...e2e-gap-room",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/reports (valid)",
    )
    .await;
    assert_eq!(filed["status"], "open");
    assert_eq!(filed["reported_id"], tgt.uid);
    let details = filed["details"].as_str().expect("details");
    assert!(!details.contains("fuck") && !details.contains("shitty"), "{details}");
    let report_id = filed["id"].as_str().expect("report id").to_owned();

    assert_eq!(
        refusal(
            env.client
                .get(reports(""))
                .bearer_auth(&rep.token)
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "GET /v2/reports with a player session",
        )
        .await,
        "Console token invalid"
    );

    let page = json_body(
        env.client
            .get(reports("?status=open&limit=100"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/reports (operator)",
    )
    .await;
    assert!(
        page["reports"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["id"] == report_id),
        "{page}"
    );

    assert_eq!(
        refusal(
            env.client
                .put(reports(&format!("/{report_id}/status")))
                .bearer_auth(console)
                .json(&json!({ "status": "resolved" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "PUT /v2/reports/{id}/status with an unknown status",
        )
        .await,
        "Invalid report status."
    );

    let room = format!("2...e2e-gap-room-{}", letters_suffix());
    let send = |text: &str| {
        env.client
            .post(format!("{}/v2/channel/{room}/message", env.base))
            .bearer_auth(rep.token.clone())
            .json(&json!({ "content": { "text": text } }))
    };
    expect(
        send("unmutated line").send().await.unwrap(),
        StatusCode::OK,
        "channel send before any mute",
    )
    .await;

    let second = json_body(
        env.client
            .post(reports(""))
            .bearer_auth(tgt.token.clone())
            .json(&json!({
                "reported_id": rep.uid,
                "reason": "cheating",
                "details": "gap second report",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/reports (second, reporter becomes the reported)",
    )
    .await;
    let second_id = second["id"].as_str().expect("second report id").to_owned();

    let actioned = json_body(
        env.client
            .put(reports(&format!("/{second_id}/status")))
            .bearer_auth(console)
            .json(&json!({
                "status": "actioned",
                "kind": "mute",
                "duration_secs": 3600,
                "note": "gap enforcement",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/reports/{id}/status (actioned with a mute)",
    )
    .await;
    assert_eq!(actioned["status"], "actioned");
    assert_eq!(
        count(
            env.db(),
            "user_infraction",
            Some("user_id = $uid AND kind = 'mute'"),
            &[("uid", json!(rep.uid))],
        )
        .await,
        1,
        "the mute was recorded against the reported player"
    );

    assert_eq!(
        refusal(
            send("muted line").send().await.unwrap(),
            StatusCode::FORBIDDEN,
            "channel send while muted",
        )
        .await,
        "You are muted and cannot send messages."
    );

    for (label, path) in [
        ("email signup", "/email"),
        ("custom signup", "/custom"),
    ] {
        let mut request = env
            .client
            .post(format!(
                "{}/v2/account/authenticate{path}?username=ab&create=true",
                env.base
            ))
            .header(reqwest::header::AUTHORIZATION, server_key_header());
        request = if path == "/email" {
            request.json(&json!({
                "email": "gap-moder@example.com",
                "password": "gap-password-1",
            }))
        } else {
            request.json(&json!({ "id": format!("e2e-gap-custom-{}", Uuid::new_v4()) }))
        };
        assert_eq!(
            refusal(
                request.send().await.unwrap(),
                StatusCode::BAD_REQUEST,
                label,
            )
            .await,
            "Username must be 3-20 characters and free of profanity.",
            "{label}"
        );
    }
}
