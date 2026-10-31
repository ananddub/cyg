use super::*;

pub async fn operator_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console account ban, notes, acl templates and operators");

    let player = device_player(&env.client, &env.base, "banee").await;

    assert_eq!(
        account_row(env, console, &player.uid).await["disable_time"],
        NEVER
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/ban",
                    env.base, player.uid
                ))
                .bearer_auth(viewer)
                .json(&json!({ "reason": "cheating" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "banning a player as a read-only operator",
        )
        .await,
        "This console user has read-only access."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/account/not-a-uuid/ban", env.base))
                .bearer_auth(console)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "banning an account that is not a uuid",
        )
        .await,
        "Invalid account id."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/ban",
                    env.base,
                    Uuid::new_v4()
                ))
                .bearer_auth(console)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "banning an account that does not exist",
        )
        .await,
        "Entity not found."
    );
    expect(
        env.client
            .post(format!(
                "{}/v2/console/account/{}/ban",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .json(&json!({ "reason": "exploiting" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/ban",
    )
    .await;
    let banned = account_row(env, console, &player.uid).await;
    assert_ne!(
        banned["disable_time"].as_str().unwrap_or_default(),
        NEVER,
        "the ban is a timestamp, not a flag: {banned}"
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/note",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .json(&json!({ "note": "   " }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a note with no text",
        )
        .await,
        "A note cannot be empty."
    );
    expect(
        env.client
            .post(format!(
                "{}/v2/console/account/{}/note",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .json(&json!({ "note": "Reviewed the exploit replay." }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/note",
    )
    .await;
    let notes = json_body(
        env.client
            .get(format!(
                "{}/v2/console/account/{}/note",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/account/{id}/note",
    )
    .await;
    let rows = notes.as_array().expect("an array of notes").clone();
    assert_eq!(rows.len(), 1, "{notes}");
    assert_eq!(rows[0]["note"], "Reviewed the exploit replay.");
    let note_id = rows[0]["id"].as_str().expect("note id").to_owned();

    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/account/{}/note/not-a-uuid",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "removing a note that is not a uuid",
        )
        .await,
        "Invalid note id."
    );
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/account/{}/note/{note_id}",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/account/{id}/note/{noteId}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/account/{}/note/{note_id}",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "removing the same note twice",
        )
        .await,
        "Note not found."
    );

    expect(
        env.client
            .post(format!(
                "{}/v2/console/account/{}/unban",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/unban",
    )
    .await;
    assert_eq!(
        account_row(env, console, &player.uid).await["disable_time"],
        NEVER,
        "the unban writes the epoch back"
    );

    let templates = json_body(
        env.client
            .get(format!("{}/v2/console/acl/template", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/acl/template",
    )
    .await;
    assert!(
        templates.is_array(),
        "the template list is an array: {templates}"
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(viewer)
                .json(&json!({ "name": "nope" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "creating a template as a read-only operator",
        )
        .await,
        "This console action requires the admin role."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "  " }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a template with no name",
        )
        .await,
        "A template needs a name."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "gap-acl-shape", "acl": ["players.read"] }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a template whose acl is not an object",
        )
        .await,
        "A template's acl must be an object."
    );
    let template_name = format!("gap acl {}", letters_suffix());
    let made = json_body(
        env.client
            .post(format!("{}/v2/console/acl/template", env.base))
            .bearer_auth(console)
            .json(&json!({
                "name": template_name,
                "description": "Read one board.",
                "acl": { "players.read": true },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/acl/template",
    )
    .await;
    let template_id = made["id"].as_str().expect("template id").to_owned();
    assert_eq!(made["name"], template_name);
    assert_eq!(made["acl"]["players.read"], true);
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": template_name }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "a second template with the same name",
        )
        .await,
        "A template with that name already exists."
    );
    let edited = json_body(
        env.client
            .put(format!(
                "{}/v2/console/acl/template/{template_id}",
                env.base
            ))
            .bearer_auth(console)
            .json(&json!({
                "name": format!("{template_name} v2"),
                "description": "Read and write one board.",
                "acl": { "players.read": true, "leaderboards.write": true },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/console/acl/template/{id}",
    )
    .await;
    assert_eq!(edited["name"], format!("{template_name} v2"));
    assert_eq!(edited["acl"]["leaderboards.write"], true);
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/console/acl/template/not-a-uuid", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "whatever" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "editing a template that is not a uuid",
        )
        .await,
        "Invalid template id."
    );
    assert_eq!(
        refusal(
            env.client
                .put(format!(
                    "{}/v2/console/acl/template/{}",
                    env.base,
                    Uuid::new_v4()
                ))
                .bearer_auth(console)
                .json(&json!({ "name": "whatever" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "editing a template that does not exist",
        )
        .await,
        "Template not found."
    );

    let ops_name = format!("e2e-cops-{}", letters_suffix());
    let ops_password = "e2e-operator-password";
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/user", env.base))
                .bearer_auth(console)
                .json(&json!({
                    "username": ops_name,
                    "email": format!("{ops_name}@example.com"),
                    "password": ops_password,
                    "role": "superuser",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a role the console does not have",
        )
        .await,
        "The role must be admin, developer, maintainer or readonly."
    );
    let operator = json_body(
        env.client
            .post(format!("{}/v2/console/user", env.base))
            .bearer_auth(console)
            .json(&json!({
                "username": ops_name,
                "email": format!("{ops_name}@example.com"),
                "password": ops_password,
                "role": "admin",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create an admin operator",
    )
    .await;
    let ops_id = operator["id"].as_str().expect("operator id").to_owned();
    assert_eq!(operator["role"], "admin");
    assert_eq!(operator["disabled"], false);

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/user/{ops_id}/password", env.base))
                .bearer_auth(console)
                .json(&json!({ "password": "short" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a password under the console minimum",
        )
        .await,
        "A console password must be at least 12 characters."
    );
    let rotated = "e2e-operator-rotated";
    expect(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/password", env.base))
            .bearer_auth(console)
            .json(&json!({ "password": rotated }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/password",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/authenticate", env.base))
                .json(&json!({ "username": ops_name, "password": ops_password }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "signing in with the replaced password",
        )
        .await,
        "Invalid credentials."
    );
    let ops = console_login_as(env, &ops_name, rotated, json!({})).await;

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/user/{ops_id}/disable", env.base))
                .bearer_auth(&ops)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an operator disabling itself",
        )
        .await,
        "A console user cannot disable itself."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/password_change", env.base))
                .bearer_auth(console)
                .json(&json!({ "current_password": rotated, "password": "another-password" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the bootstrap account changing its own password",
        )
        .await,
        "The bootstrap console account has no password to change."
    );
    refusal(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/disable", env.base))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "disabling an operator as a read-only operator",
    )
    .await;
    expect(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/disable", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/disable",
    )
    .await;
    let staff = json_body(
        env.client
            .get(format!("{}/v2/console/user?limit=100", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the operator roster",
    )
    .await;
    let listed = staff
        .as_array()
        .expect("an array of operators")
        .iter()
        .find(|row| row["id"] == json!(ops_id))
        .cloned()
        .unwrap_or_else(|| panic!("the operator is listed: {staff}"));
    assert_eq!(listed["disabled"], true, "the roster says it is off");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/authenticate", env.base))
                .json(&json!({ "username": ops_name, "password": rotated }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "signing in as a disabled operator",
        )
        .await,
        "Invalid credentials."
    );
    expect(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/enable", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/enable",
    )
    .await;
    console_login_as(env, &ops_name, rotated, json!({})).await;

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/password_change", env.base))
                .bearer_auth(&ops)
                .json(&json!({ "current_password": "not-it-at-all", "password": rotated }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "changing a password with the wrong current one",
        )
        .await,
        "Current password is incorrect."
    );
    let own = format!("{template_name}-pw");
    expect(
        env.client
            .post(format!("{}/v2/console/password_change", env.base))
            .bearer_auth(&ops)
            .json(&json!({ "current_password": rotated, "password": own }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/password_change",
    )
    .await;
    console_login_as(env, &ops_name, own.as_str(), json!({})).await;

    expect(
        env.client
            .delete(format!(
                "{}/v2/console/acl/template/{template_id}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/acl/template/{id}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/acl/template/{template_id}",
                    env.base
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting the same template twice",
        )
        .await,
        "Template not found."
    );
}
