use super::*;

pub async fn console_account_lifecycle(env: &Harness, console: &str) {
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
    let gone = count(
        env.db(),
        "users",
        Some("id = type::record('users', <uuid>$user)"),
        &[("user", json!(uid))],
    )
    .await;
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
    let restored: Option<String> = scalar(
        env.db(),
        "SELECT VALUE username FROM users \
         WHERE id = type::record('users', <uuid>$user) LIMIT 1",
        &[("user", json!(uid))],
    )
    .await;
    assert_eq!(
        restored.as_deref(),
        Some(player.username.as_str()),
        "the imported account is the one that was exported"
    );
    let notes = count(
        env.db(),
        "users_notes",
        Some("(user_id = <uuid>$user OR user_id = $user)"),
        &[("user", json!(uid))],
    )
    .await;
    assert_eq!(notes, 1, "the exported note came back");
    let credits = count(
        env.db(),
        "wallet_ledger",
        Some("(user_id = <uuid>$user OR user_id = $user)"),
        &[("user", json!(uid))],
    )
    .await;
    assert_eq!(credits, 1, "the exported ledger came back");

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
