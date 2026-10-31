use super::*;

pub async fn console_operator_lifecycle(env: &Harness, console: &str) {
    eprintln!("gap: console operator lifecycle");
    let username = format!("e2e-op-{}", letters_suffix());
    let email = format!("{username}@example.com");
    let password = "e2e-operator-password";
    let changed = "e2e-operator-password-2";

    let created = json_body(
        env.client
            .post(format!("{}/v2/console/user", env.base))
            .bearer_auth(console)
            .json(&json!({
                "username": username,
                "email": email,
                "password": password,
                "role": "admin",
                "mfa_required": false,
            }))
            .send()
            .await
            .expect("create a console operator"),
        StatusCode::OK,
        "POST /v2/console/user",
    )
    .await;
    let operator_id = created["id"].as_str().expect("operator id").to_owned();
    assert_eq!(created["role"], "admin");
    assert_eq!(created["mfa_required"], false);

    let operator = console_login_as(env, &username, password, json!({})).await;

    refusal(
        env.client
            .post(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "MFA enrolment for the bootstrap account",
    )
    .await;

    let enrolment = json_body(
        env.client
            .post(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(&operator)
            .send()
            .await
            .expect("enrol MFA"),
        StatusCode::OK,
        "POST /v2/console/mfa",
    )
    .await;
    let secret = enrolment["secret"]
        .as_str()
        .expect("base32 secret")
        .to_owned();
    assert!(
        enrolment["uri"]
            .as_str()
            .unwrap_or_default()
            .starts_with("otpauth://totp/"),
        "the enrolment hands out a provisioning URI: {enrolment}"
    );
    let recovery = enrolment["recovery_codes"]
        .as_array()
        .expect("recovery codes");
    assert!(
        !recovery.is_empty(),
        "recovery codes are shown at enrolment"
    );

    refusal(
        env.client
            .post(format!("{}/v2/console/mfa/verify", env.base))
            .bearer_auth(&operator)
            .json(&json!({"code": "000000"}))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "MFA verification with a stale code",
    )
    .await;
    let code = totp::code_at(&secret, chrono::Utc::now().timestamp()).expect("a TOTP");
    expect(
        env.client
            .post(format!("{}/v2/console/mfa/verify", env.base))
            .bearer_auth(&operator)
            .json(&json!({"code": code}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/mfa/verify with the live code",
    )
    .await;

    refusal(
        env.client
            .post(format!("{}/v2/console/authenticate", env.base))
            .json(&json!({"username": username, "password": password}))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console sign-in without the second factor",
    )
    .await;
    let code = totp::code_at(&secret, chrono::Utc::now().timestamp()).expect("a TOTP");
    let signed_in = json_body(
        env.client
            .post(format!("{}/v2/console/authenticate", env.base))
            .json(&json!({"username": username, "password": password, "mfa": code}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "console sign-in with the second factor",
    )
    .await;
    let operator = signed_in["token"]
        .as_str()
        .expect("operator token")
        .to_owned();

    refusal(
        env.client
            .post(format!("{}/v2/console/password_change", env.base))
            .bearer_auth(&operator)
            .json(&json!({"current_password": "wrong-password", "password": changed}))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "password change with the wrong current password",
    )
    .await;
    refusal(
        env.client
            .post(format!("{}/v2/console/password_change", env.base))
            .bearer_auth(&operator)
            .json(&json!({"current_password": password, "password": "short"}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "password change below the length floor",
    )
    .await;
    let bootstrap = json_body(
        env.client
            .post(format!("{}/v2/console/password_change", env.base))
            .bearer_auth(console)
            .json(&json!({"current_password": CONSOLE_PASSWORD, "password": changed}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "password change as the bootstrap account",
    )
    .await;
    assert!(
        bootstrap["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("bootstrap"),
        "the refusal names the bootstrap account: {bootstrap}"
    );
    let code = totp::code_at(&secret, chrono::Utc::now().timestamp()).expect("a TOTP");
    expect(
        env.client
            .post(format!("{}/v2/console/password_change", env.base))
            .bearer_auth(&operator)
            .json(&json!({"current_password": password, "password": changed, "mfa": code}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/password_change",
    )
    .await;
    let _ = console_login_as(
        env,
        &username,
        changed,
        json!({"mfa": secret_code(&secret)}),
    )
    .await;

    let first_recovery = recovery[0].as_str().expect("a recovery code").to_owned();
    expect(
        env.client
            .delete(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(&operator)
            .json(&json!({"code": first_recovery}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/mfa with a recovery code",
    )
    .await;
    expect(
        env.client
            .delete(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(&operator)
            .json(&json!({"code": first_recovery}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "repeating the disable once the factor is off",
    )
    .await;
    let operator_uuid = operator_id
        .parse::<uuid::Uuid>()
        .expect("operator id is a uuid");
    let secret_left: Option<Value> = scalar(
        env.db(),
        "SELECT VALUE mfa_secret FROM type::record('console_user', <uuid>$id) LIMIT 1",
        &[("id", json!(operator_uuid.to_string()))],
    )
    .await;
    assert!(
        matches!(secret_left, None | Some(Value::Null)),
        "disabling wipes the secret from the row"
    );
    let codes_left: Option<Value> = scalar(
        env.db(),
        "SELECT VALUE mfa_recovery_codes FROM type::record('console_user', <uuid>$id) LIMIT 1",
        &[("id", json!(operator_uuid.to_string()))],
    )
    .await;
    assert!(
        matches!(codes_left, None | Some(Value::Null)),
        "disabling wipes the recovery codes from the row"
    );
    let required_left: Option<Value> = scalar(
        env.db(),
        "SELECT VALUE mfa_required FROM type::record('console_user', <uuid>$id) LIMIT 1",
        &[("id", json!(operator_uuid.to_string()))],
    )
    .await;
    assert_eq!(
        required_left,
        Some(json!(false)),
        "disabling clears the requirement flag"
    );

    let re_enrolment = json_body(
        env.client
            .post(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(&operator)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "re-enrolling MFA after the disable",
    )
    .await;
    let new_secret = re_enrolment["secret"]
        .as_str()
        .expect("a second MFA secret")
        .to_owned();
    refusal(
        env.client
            .delete(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(&operator)
            .json(&json!({"code": first_recovery}))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "a recovery code from a superseded enrolment",
    )
    .await;
    expect(
        env.client
            .delete(format!("{}/v2/console/mfa", env.base))
            .bearer_auth(&operator)
            .json(&json!({"code": secret_code(&new_secret)}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/mfa with the live authenticator code",
    )
    .await;

    expect(
        env.client
            .post(format!(
                "{}/v2/console/user/{operator_id}/mfa/require",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/mfa/require",
    )
    .await;
    let required = json_body(
        env.client
            .get(format!("{}/v2/console/user", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the operator roster after requiring MFA",
    )
    .await;
    let row = required["users"]
        .as_array()
        .map(|rows| rows.as_slice())
        .unwrap_or(&[])
        .iter()
        .chain(
            required
                .as_array()
                .map(|rows| rows.as_slice())
                .unwrap_or(&[])
                .iter(),
        )
        .find(|u| u["id"].as_str() == Some(operator_id.as_str()))
        .cloned()
        .unwrap_or_else(|| panic!("the created operator is listed: {required}"));
    assert_eq!(row["mfa_required"], true, "the flag is stored");

    expect(
        env.client
            .delete(format!(
                "{}/v2/console/user/{operator_id}/mfa/require",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/user/{id}/mfa/require",
    )
    .await;
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/user/{operator_id}/mfa/reset",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/user/{id}/mfa/reset",
    )
    .await;
    refusal(
        env.client
            .post(format!(
                "{}/v2/console/user/{}/mfa/require",
                env.base,
                Uuid::new_v4()
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "requiring MFA for an unknown operator",
    )
    .await;
    refusal(
        env.client
            .post(format!(
                "{}/v2/console/user/not-a-uuid/mfa/require",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "requiring MFA with a malformed id",
    )
    .await;

    let viewer_name = format!("e2e-view-{}", letters_suffix());
    let viewer_password = "e2e-viewer-password";
    json_body(
        env.client
            .post(format!("{}/v2/console/user", env.base))
            .bearer_auth(console)
            .json(&json!({
                "username": viewer_name,
                "email": format!("{viewer_name}@example.com"),
                "password": viewer_password,
                "role": "readonly",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a read-only operator",
    )
    .await;
    let viewer = console_login_as(env, &viewer_name, viewer_password, json!({})).await;
    let message = refusal(
        env.client
            .post(format!(
                "{}/v2/console/user/{operator_id}/mfa/require",
                env.base
            ))
            .bearer_auth(&viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "an admin route driven by a read-only operator",
    )
    .await;
    assert_eq!(message, "This console action requires the admin role.");

    let trail = json_body(
        env.client
            .get(format!("{}/v2/console/audit/log?limit=100", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/audit/log",
    )
    .await;
    let actions: Vec<&str> = trail
        .as_array()
        .map(|rows| rows.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|row| row["action"].as_str())
        .collect();
    for action in [
        "console_user.create",
        "mfa.enrol",
        "mfa.confirm",
        "mfa.disable",
        "console_user.password_change",
        "console_user.mfa.require",
        "console_user.mfa.unrequire",
        "console_user.mfa.reset",
    ] {
        assert!(
            actions.contains(&action),
            "{action} is in the trail: {actions:?}"
        );
    }
    let filtered = json_body(
        env.client
            .get(format!(
                "{}/v2/console/audit/log?username={username}&action=mfa.enrol",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the audit trail filtered by operator and action",
    )
    .await;
    let enrolments = filtered
        .as_array()
        .map(|rows| rows.as_slice())
        .unwrap_or(&[])
        .to_vec();
    assert_eq!(
        enrolments.len(),
        2,
        "the operator enrolled twice and both rows are in the trail: {filtered}"
    );
    for row in &enrolments {
        assert_eq!(
            (row["action"].as_str(), row["console_username"].as_str()),
            (Some("mfa.enrol"), Some(username.as_str())),
            "the filtered trail holds only this operator's enrolments: {row}"
        );
    }

    let roster = json_body(
        env.client
            .get(format!("{}/v2/console/audit/log/users", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/audit/log/users",
    )
    .await;
    let usernames: Vec<&str> = roster["usernames"]
        .as_array()
        .expect("the roster is a list of names")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        usernames.contains(&username.as_str()),
        "the operator who acted appears in the roster: {usernames:?}"
    );
    assert!(
        usernames.contains(&CONSOLE_USER),
        "the bootstrap superadmin appears too: {usernames:?}"
    );

    expect(
        env.client
            .post(format!("{}/v2/console/authenticate/logout", env.base))
            .bearer_auth(&operator)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/authenticate/logout",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/console/status", env.base))
                .bearer_auth(&operator)
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a console call after logout",
        )
        .await,
        "Console token invalid"
    );
}
