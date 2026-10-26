//! The HTTP surface `rest_e2e.rs` never reaches, end to end against the same
//! Testcontainers plane: the console operator lifecycle (MFA, own password,
//! audit roster, per-user MFA administration), account export/import and wallet
//! ledger deletion, group export, the console match registry, the leaderboard
//! record deletion the console owns, the dashboard identity route, the static
//! file surface, and the success paths of the player routes that were only
//! ever probed for registration (`PUT/DELETE /v2/account`, `POST /v2/user`,
//! `PUT/DELETE /v2/group`, `GET /v2/match*`).
//!
//! One test, one server, one database: the plane installs its signing keys and
//! revocation caches process-wide, so a single lifecycle keeps the assertions
//! honest about each other.

use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness;

use harness::*;

use engine_api::services::realtime::RealtimeMatchService;
use engine_api::utils::totp;

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

async fn dashboard_identity(env: &Harness) {
    eprintln!("gap: dashboard identity");
    // Public route: it answers without a header at all.
    let body = json_body(
        env.client
            .get(format!("{}/api/dashboard/identity", env.base))
            .send()
            .await
            .expect("dashboard identity"),
        StatusCode::OK,
        "GET /api/dashboard/identity",
    )
    .await;
    assert!(
        body.get("game_id").is_some() && body.get("game_name").is_some(),
        "the identity document names the game keys: {body}"
    );
}

async fn static_surface(env: &Harness) {
    eprintln!("gap: static file surface");
    // The website root redirects to the dashboard; the client follows it, so
    // the honest assertion is where the call ended up.
    let resp = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .expect("root");
    assert_eq!(resp.status(), StatusCode::OK, "GET / follows its redirect");
    assert_eq!(
        resp.url().path(),
        "/dashboard/",
        "GET / lands on the dashboard shell"
    );

    // Every named asset the crate directory does not carry is a clean 404.
    for path in [
        "/cyg.js",
        "/openoxide-client.js",
        "/arena_map.json",
        "/packages/sdk/dist/colyseus.js",
        "/node_modules/@colyseus/schema/build/index.mjs",
        "/unity",
        "/unity/",
        "/unity/index.html",
        "/Build/webgl.wasm.framework.unityweb",
        "/TemplateData/style.css",
    ] {
        let resp = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap_or_else(|err| panic!("GET {path}: {err}"));
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "GET {path} answers 404 while the file is absent"
        );
        let body = resp.text().await.unwrap_or_default();
        assert_eq!(body, "404 not found", "GET {path} body: {body}");
    }

    // A fixture that exists on disk is served with its own MIME type.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = dir.join("arena_map.json");
    std::fs::write(&fixture, json!({"size": 2048}).to_string()).expect("write the fixture");
    let resp = env
        .client
        .get(format!("{}/arena_map.json", env.base))
        .send()
        .await
        .expect("GET /arena_map.json");
    let outcome = if resp.status() == StatusCode::OK {
        assert_eq!(
            resp.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default(),
            "application/json",
            "the JSON fixture is typed as JSON"
        );
        "served"
    } else {
        // The fallback resolver may hand the tree elsewhere; either way the
        // route answered, and the file is cleaned up below.
        "resolved elsewhere"
    };
    std::fs::remove_file(&fixture).expect("remove the fixture");
    eprintln!("gap: static fixture outcome: {outcome}");
}

/// The console operator lifecycle: a real operator row, MFA enrolment verified
/// with a TOTP the same module the server uses, a self-service password change,
/// the administrator's MFA switches and the audit trail they all write.
async fn console_operator_lifecycle(env: &Harness, console: &str) {
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

    // The bootstrap superadmin has no row of its own, so it cannot enrol.
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

    // A wrong code is refused; the live window's code confirms.
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

    // Once a secret exists a sign-in without a code is refused.
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

    // The operator changes their own password; the old one stops working.
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
    // The bootstrap account has no password to change at all.
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

    // Disabling MFA with a recovery code consumes it.
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
    // Disabling a factor that is already off is idempotent: the row has no secret and
    // no requirement left, so the second-factor check has nothing to refuse.
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
    let secret_left: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT mfa_secret FROM console_user WHERE id = $1")
            .bind(operator_uuid)
            .fetch_one(&env.pool)
            .await
            .expect("read the operator's MFA secret");
    assert!(
        secret_left.is_none(),
        "disabling wipes the secret from the row"
    );
    let codes_left: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT mfa_recovery_codes FROM console_user WHERE id = $1")
            .bind(operator_uuid)
            .fetch_one(&env.pool)
            .await
            .expect("read the operator's recovery codes");
    assert!(
        codes_left.is_none(),
        "disabling wipes the recovery codes from the row"
    );
    let required_left: Option<bool> =
        sqlx::query_scalar("SELECT mfa_required FROM console_user WHERE id = $1")
            .bind(operator_uuid)
            .fetch_one(&env.pool)
            .await
            .expect("read the operator's MFA requirement");
    assert_eq!(
        required_left,
        Some(false),
        "disabling clears the requirement flag"
    );

    // A recovery code belongs to the enrolment that minted it. Re-enrolling replaces
    // the stored hashes, so the spent code from the previous enrolment verifies
    // against nothing and the disable is refused — until the fresh authenticator
    // code, which also covers turning the factor off without a recovery code.
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

    // The administrator's MFA switches on the operator row.
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

    // A read-only operator is refused by the same routes.
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

    // The audit trail records every action above, and the roster names them.
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

    // Logout revokes the token by its id; the next call is refused.
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

/// The code for the current window, read from the secret the server handed out.
fn secret_code(secret: &str) -> String {
    totp::code_at(secret, chrono::Utc::now().timestamp()).expect("a TOTP for the live window")
}

async fn console_login_as(env: &Harness, username: &str, password: &str, extra: Value) -> String {
    let mut body = json!({ "username": username, "password": password });
    if let Value::Object(map) = extra {
        for (key, value) in map {
            body[key] = value;
        }
    }
    let reply = json_body(
        env.client
            .post(format!("{}/v2/console/authenticate", env.base))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("sign in as {username}: {err}")),
        StatusCode::OK,
        &format!("console sign-in as {username}"),
    )
    .await;
    reply["token"].as_str().expect("token").to_owned()
}

/// Account export, deletion and re-import, plus the wallet ledger delete.
async fn console_account_lifecycle(env: &Harness, console: &str) {
    eprintln!("gap: console account export/import and wallet ledger");
    let player = device_player(&env.client, &env.base, "export").await;
    let uid = Uuid::parse_str(&player.uid).expect("player id");

    expect(
        env.client
            .post(format!("{}/v2/console/account/{uid}/note", env.base))
            .bearer_auth(console)
            .json(&json!({"note": "exported for the round trip"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "leave a note on the account",
    )
    .await;
    expect(
        env.client
            .post(format!("{}/v2/console/account/{uid}/wallet", env.base))
            .bearer_auth(console)
            .json(&json!({"changeset": {"gold": 250}, "metadata": {"reason": "e2e"}}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "credit the wallet",
    )
    .await;

    let ledger = json_body(
        env.client
            .get(format!("{}/v2/console/account/{uid}/wallet", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/account/{id}/wallet",
    )
    .await;
    let rows = ledger.as_array().expect("the ledger is a list").clone();
    assert_eq!(rows.len(), 1, "one credit was written: {ledger}");
    let ledger_id = rows[0]["id"].as_str().expect("ledger row id").to_owned();

    let exported = json_body(
        env.client
            .get(format!("{}/v2/console/account/{uid}/export", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/account/{id}/export",
    )
    .await;
    assert_eq!(exported["account"]["id"], player.uid);
    assert_eq!(exported["account"]["username"], player.username);
    assert_eq!(exported["devices"].as_array().map(Vec::len), Some(1));
    assert_eq!(exported["notes"].as_array().map(Vec::len), Some(1));
    assert_eq!(exported["wallet_ledger"].as_array().map(Vec::len), Some(1));

    // The path id must name the account in the body.
    let mismatched = {
        let mut clone = exported.clone();
        clone["account"]["id"] = json!(Uuid::new_v4().to_string());
        clone
    };
    refusal(
        env.client
            .post(format!("{}/v2/console/account/{uid}/import", env.base))
            .bearer_auth(console)
            .json(&mismatched)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "importing a document for a different id",
    )
    .await;

    // Deleting the wallet ledger entry takes the row out of the trail.
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/account/{uid}/wallet/{ledger_id}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/account/{id}/wallet/{wallet_id}",
    )
    .await;
    let after = json_body(
        env.client
            .get(format!("{}/v2/console/account/{uid}/wallet", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the ledger after deletion",
    )
    .await;
    assert_eq!(after.as_array().map(Vec::len), Some(0));
    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/account/{uid}/wallet/{ledger_id}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "deleting the same ledger row twice",
    )
    .await;

    // Round trip: the document exported while the ledger row still existed is the
    // import body verbatim, so deleting the account also tests that the import
    // restores the note and the ledger row it carries.
    let document = exported.clone();
    expect(
        env.client
            .delete(format!("{}/v2/console/account/{uid}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the account",
    )
    .await;
    let gone: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = $1")
        .bind(uid)
        .fetch_one(&env.pool)
        .await
        .expect("count the users table");
    assert_eq!(gone, 0, "the account is gone before the import");

    expect(
        env.client
            .post(format!("{}/v2/console/account/{uid}/import", env.base))
            .bearer_auth(console)
            .json(&document)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/import",
    )
    .await;
    let restored: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(uid)
        .fetch_one(&env.pool)
        .await
        .ok();
    assert_eq!(
        restored.as_deref(),
        Some(player.username.as_str()),
        "the imported account is the one that was exported"
    );
    let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users_notes WHERE user_id = $1")
        .bind(uid)
        .fetch_one(&env.pool)
        .await
        .expect("count the notes");
    assert_eq!(notes, 1, "the exported note came back");
    let credits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wallet_ledger WHERE user_id = $1")
        .bind(uid)
        .fetch_one(&env.pool)
        .await
        .expect("count the ledger");
    assert_eq!(credits, 1, "the exported ledger came back");

    // Re-importing an account that is already alive is a conflict.
    refusal(
        env.client
            .post(format!("{}/v2/console/account/{uid}/import", env.base))
            .bearer_auth(console)
            .json(&document)
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "importing an account that already exists",
    )
    .await;
    let trail = json_body(
        env.client
            .get(format!(
                "{}/v2/console/audit/log?action=account.import&limit=20",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the audit trail for the import",
    )
    .await;
    assert!(
        trail
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .iter()
            .any(|row| row["action"] == "account.import"),
        "the import is audited: {trail}"
    );
}

/// The console group export, fed by a group the player surface created.
async fn console_group_export(env: &Harness, console: &str) -> String {
    eprintln!("gap: console group export");
    let owner = device_player(&env.client, &env.base, "group").await;
    let name = format!("e2e group {}", letters_suffix());
    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&owner.token)
            .json(&json!({
                "name": name,
                "open": true,
                "max_count": 12,
                "description": "exported from the console",
                "metadata": "{\"tier\":\"gold\"}",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/group",
    )
    .await;
    let group_id = created["group"]["id"]
        .as_str()
        .expect("group id")
        .to_owned();

    let exported = json_body(
        env.client
            .get(format!("{}/v2/console/group/{group_id}/export", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/group/{id}/export",
    )
    .await;
    assert_eq!(exported["group"]["id"], group_id);
    assert_eq!(exported["group"]["name"], name);
    assert_eq!(
        json_string(&exported["group"]["metadata"]),
        json!({"tier": "gold"})
    );
    let members = exported["members"]
        .as_array()
        .expect("the export lists members");
    assert_eq!(members.len(), 1, "only the creator is a member");
    assert_eq!(members[0]["id"], owner.uid);
    assert_eq!(members[0]["state"], "superadmin");
    assert_eq!(members[0]["username"], owner.username);

    refusal(
        env.client
            .get(format!(
                "{}/v2/console/group/{}/export",
                env.base,
                Uuid::new_v4()
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "exporting a group that does not exist",
    )
    .await;

    // The leader's own update and delete paths, which `rest_e2e.rs` only ever
    // watched refuse.
    expect(
        env.client
            .put(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .json(&json!({"description": "renamed by its leader", "open": false}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/group/{id} as the leader",
    )
    .await;
    let fetched = json_body(
        env.client
            .get(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{id} after the update",
    )
    .await;
    assert_eq!(fetched["group"]["open"], false);
    assert_eq!(
        fetched["group"]["description"], "renamed by its leader",
        "the leader's edit is stored: {fetched}"
    );

    expect(
        env.client
            .delete(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/group/{id} as the leader",
    )
    .await;
    refusal(
        env.client
            .get(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "reading a deleted group",
    )
    .await;
    group_id
}

/// The console match registry, fed by the same in-process service the sockets
/// write to.
async fn console_match_surface(env: &Harness, console: &str) {
    eprintln!("gap: console match registry");
    let match_id = RealtimeMatchService::create_match_with_tokens(15, vec!["e2e-token".into()]);
    let listed = json_body(
        env.client
            .get(format!("{}/v2/console/match", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/match",
    )
    .await;
    let rows = listed["matches"]
        .as_array()
        .expect("the console list wraps its rows")
        .clone();
    let row = rows
        .iter()
        .find(|r| r["api_match"]["match_id"] == match_id.simple().to_string())
        .unwrap_or_else(|| panic!("the seeded match is listed: {listed}"));
    assert_eq!(row["api_match"]["tick_rate"], 15);
    assert_eq!(row["api_match"]["authoritative"], false);
    assert!(row["node"].is_string(), "every row names its node");

    let state = json_body(
        env.client
            .get(format!("{}/v2/console/match/{match_id}/state", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/match/{id}/state",
    )
    .await;
    assert!(state["presences"].is_array(), "the state names presences");
    assert_eq!(state["tick"], 0, "a relay match does not tick");

    refusal(
        env.client
            .get(format!("{}/v2/console/match/not-a-uuid/state", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a malformed match id in the state route",
    )
    .await;
    refusal(
        env.client
            .delete(format!("{}/v2/console/match/{}", env.base, Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "terminating a match that never existed",
    )
    .await;

    expect(
        env.client
            .delete(format!("{}/v2/console/match/{match_id}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/match/{id}",
    )
    .await;
    assert!(
        !RealtimeMatchService::check_match_exists(match_id),
        "the registry dropped the terminated match"
    );
    let trail = json_body(
        env.client
            .get(format!(
                "{}/v2/console/audit/log?action=match.terminate&limit=20",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the audit trail for the termination",
    )
    .await;
    assert!(
        trail
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .iter()
            .any(|row| row["action"] == "match.terminate"),
        "the termination is audited: {trail}"
    );
}

/// The console's deletion of one player's record from a plain leaderboard.
async fn console_leaderboard_record_delete(env: &Harness, console: &str) {
    eprintln!("gap: console leaderboard record deletion");
    let player = device_player(&env.client, &env.base, "board").await;
    let board = format!("e2e-console-lb-{}", letters_suffix());
    sqlx::query(
        "INSERT INTO leaderboard (id, title, category, duration, sort_order, authoritative) \
         VALUES ($1, $1, 0, 0, 1, false)",
    )
    .bind(&board)
    .execute(&env.pool)
    .await
    .expect("seed a plain board");
    json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"score": 42}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "write the player's record",
    )
    .await;
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM leaderboard_record WHERE leaderboard_id = $1 AND owner_id = $2",
    )
    .bind(&board)
    .bind(Uuid::parse_str(&player.uid).expect("uid"))
    .fetch_one(&env.pool)
    .await
    .expect("count the record");
    assert_eq!(rows, 1, "the write is stored");

    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/leaderboard/{board}/owner/not-a-uuid",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a malformed owner id",
    )
    .await;

    expect(
        env.client
            .delete(format!(
                "{}/v2/console/leaderboard/{board}/owner/{}",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/leaderboard/{id}/owner/{owner_id}",
    )
    .await;
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM leaderboard_record WHERE leaderboard_id = $1 AND owner_id = $2",
    )
    .bind(&board)
    .bind(Uuid::parse_str(&player.uid).expect("uid"))
    .fetch_one(&env.pool)
    .await
    .expect("count the record again");
    assert_eq!(rows, 0, "the record is deleted");

    // A tournament board is not a leaderboard for this route.
    let tournament = format!("e2e-console-tb-{}", letters_suffix());
    sqlx::query(
        "INSERT INTO leaderboard (id, title, category, duration, start_time, end_time) \
         VALUES ($1, $1, 1, 60, now(), now() + interval '1 hour')",
    )
    .bind(&tournament)
    .execute(&env.pool)
    .await
    .expect("seed a tournament board");
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/leaderboard/{tournament}/owner/{}",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting a record from a tournament board",
        )
        .await,
        "Leaderboard not found."
    );
}

/// The player routes whose success paths no earlier suite drives.
async fn player_surface(env: &Harness) {
    eprintln!("gap: player account, user lookup, match read side");
    let player = device_player(&env.client, &env.base, "player").await;

    // `POST /v2/user` is the query-by-body twin of the GET: one entry per requested
    // token, so naming the same account twice by two keys answers twice.
    let by_body = json_body(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"ids": [player.uid], "usernames": [player.username]}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/user",
    )
    .await;
    let users = by_body["users"].as_array().expect("a user list").clone();
    assert_eq!(users.len(), 2, "one entry per requested token: {users:?}");
    assert!(
        users.iter().all(|user| user["id"] == player.uid),
        "both tokens name this account: {users:?}"
    );

    let only_ids = json_body(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"ids": [player.uid]}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/user by id alone",
    )
    .await;
    let found = only_ids["users"].as_array().expect("a user list").clone();
    assert_eq!(found.len(), 1, "the id resolves one account: {found:?}");
    assert_eq!(found[0]["id"], player.uid);
    assert_eq!(found[0]["username"], player.username);
    let nothing = json_body(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/user with an empty body",
    )
    .await;
    assert_eq!(
        nothing["users"].as_array().map(Vec::len),
        Some(0),
        "nothing requested is nothing returned: {nothing}"
    );
    refusal(
        env.client
            .post(format!("{}/v2/user", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"ids": ["not-a-uuid"]}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "POST /v2/user with a malformed id",
    )
    .await;

    // Profile update, answered with the whole account document.
    let updated = json_body(
        env.client
            .put(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .json(&json!({
                "display_name": "Gap Runner",
                "avatar_url": "https://example.test/avatar.png",
                "lang_tag": "en-GB",
                "location": "Deventer",
                "timezone": "Europe/Amsterdam",
                "metadata": "{\"gap\":true}",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/account",
    )
    .await;
    assert_eq!(updated["display_name"], "Gap Runner");
    assert_eq!(updated["timezone"], "Europe/Amsterdam");
    assert_eq!(json_string(&updated["metadata"]), json!({"gap": true}));
    assert_eq!(updated["user_id"], player.uid);
    assert_eq!(
        updated["device_accounts"].as_array().map(Vec::len),
        Some(1),
        "the device that signed in is listed"
    );

    // A custom id is unique across accounts.
    let custom = format!("gap-custom-{}", letters_suffix());
    expect(
        env.client
            .put(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"custom_id": custom}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "claiming a custom id",
    )
    .await;
    let other = device_player(&env.client, &env.base, "player2").await;
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/account", env.base))
                .bearer_auth(&other.token)
                .json(&json!({"custom_id": custom}))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claiming a taken custom id",
        )
        .await,
        "custom id already in use by another account"
    );
    refusal(
        env.client
            .put(format!("{}/v2/account", env.base))
            .bearer_auth(&other.token)
            .json(&json!({"metadata": "{not json"}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "account metadata that does not parse",
    )
    .await;

    // The read side of the realtime registry: nothing is queued here, and the
    // routes say so rather than failing.
    let matches = json_body(
        env.client
            .get(format!("{}/v2/match", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/match",
    )
    .await;
    assert!(
        matches["matches"].is_null() || matches["matches"].as_array() == Some(&Vec::new()),
        "the empty registry is omitted, not invented: {matches}"
    );
    refusal(
        env.client
            .get(format!("{}/v2/match?min_size=-1", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "GET /v2/match rejects a negative size",
    )
    .await;
    let stats = json_body(
        env.client
            .get(format!("{}/v2/matchmaker/stats", env.base))
            .bearer_auth(&other.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/matchmaker/stats",
    )
    .await;
    assert_eq!(stats["ticket_count"], 0);

    // The session survives the deletes above and dies with the account.
    let deleted = json_body(
        env.client
            .get(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/account before deletion",
    )
    .await;
    assert_eq!(deleted["display_name"], "Gap Runner");
    expect(
        env.client
            .delete(format!("{}/v2/account", env.base))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/account",
    )
    .await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = $1")
        .bind(Uuid::parse_str(&player.uid).expect("uid"))
        .fetch_one(&env.pool)
        .await
        .expect("count the users table");
    assert_eq!(rows, 0, "the account row is gone");
    let tombstones: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_tombstone WHERE user_id = $1")
            .bind(Uuid::parse_str(&player.uid).expect("uid"))
            .fetch_one(&env.pool)
            .await
            .expect("count the tombstone");
    assert_eq!(tombstones, 1, "the tombstone remembers the deletion");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn console_and_player_gap_surface_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    dashboard_identity(&env).await;
    static_surface(&env).await;
    console_operator_lifecycle(&env, &console).await;
    console_account_lifecycle(&env, &console).await;
    let _group_id = console_group_export(&env, &console).await;
    console_match_surface(&env, &console).await;
    console_leaderboard_record_delete(&env, &console).await;
    player_surface(&env).await;

    // Give the plane a beat to flush its audit writes behind the last calls.
    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: every uncovered HTTP surface passed");
}
