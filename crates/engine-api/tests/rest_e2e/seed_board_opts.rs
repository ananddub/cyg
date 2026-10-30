use super::*;

#[allow(clippy::too_many_arguments)]
pub async fn seed_board_opts(
    pool: &Datastore,
    id: &str,
    sort_order: i16,
    authoritative: bool,
    enable_ranks: bool,
    category: i16,
    duration: i64,
    tournament: bool,
) {
    let now = Utc::now();
    let category = if tournament { 1 } else { category };
    let duration = if tournament { 60 } else { duration };
    let end_time = if tournament {
        engine_api::db::time::encode(now + ChronoDuration::hours(1))
    } else {
        engine_api::db::time::EPOCH.to_owned()
    };
    exec(
        pool,
        "CREATE type::record('leaderboard', $id) CONTENT $row RETURN NOTHING",
        &[
            ("id", json!(id)),
            (
                "row",
                json!({
                    "authoritative": authoritative,
                    "sort_order": sort_order,
                    "operator": 0,
                    "reset_schedule": Value::Null,
                    "metadata": {},
                    "create_time": engine_api::db::time::encode(now),
                    "category": category,
                    "description": "",
                    "duration": duration,
                    "end_time": end_time,
                    "join_required": false,
                    "max_size": 100_000_000,
                    "max_num_score": 1_000_000,
                    "title": id,
                    "size": 0,
                    "start_time": engine_api::db::time::encode(now),
                    "enable_ranks": enable_ranks,
                    "game_id": Value::Null,
                }),
            ),
        ],
    )
    .await;
}
pub async fn set_disable(pool: &Datastore, uid: &str, when: &str) {
    exec(
        pool,
        "UPDATE type::record('users', <uuid>$id) SET disable_time = $when RETURN NOTHING",
        &[("id", json!(uid)), ("when", json!(when))],
    )
    .await;
}
pub async fn board_size(pool: &Datastore, id: &str) -> i64 {
    scalar::<i64>(
        pool,
        "SELECT VALUE size FROM type::record('leaderboard', $id) LIMIT 1",
        &[("id", json!(id))],
    )
    .await
    .unwrap_or(-1)
}
pub async fn seed_ledger(pool: &Datastore, id: Uuid, user_id: &str, changeset: Value) {
    let now = engine_api::db::time::encode(Utc::now());
    exec(
        pool,
        "CREATE type::record('wallet_ledger', <uuid>$id) CONTENT $row RETURN NOTHING",
        &[
            ("id", json!(id.to_string())),
            (
                "row",
                json!({
                    "user_id": user_id,
                    "changeset": changeset,
                    "metadata": {},
                    "create_time": now,
                    "update_time": now,
                }),
            ),
        ],
    )
    .await;
}
pub async fn expect(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let actual = resp.status();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        actual, status,
        "{what}: expected {status}, got {actual}: {body}"
    );
    body
}
pub async fn json_body(resp: reqwest::Response, status: StatusCode, what: &str) -> Value {
    let body = expect(resp, status, what).await;
    serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("{what}: response is not JSON ({err}): {body}"))
}
pub async fn refusal(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let body = json_body(resp, status, what).await;
    body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{what}: refusal has no message: {body}"))
        .to_owned()
}
pub async fn console_login(client: &Client, base: &str) -> String {
    let body = json_body(
        client
            .post(format!("{base}/v2/console/authenticate"))
            .json(&json!({ "username": CONSOLE_USER, "password": CONSOLE_PASSWORD }))
            .send()
            .await
            .expect("console sign-in"),
        StatusCode::OK,
        "console sign-in",
    )
    .await;
    assert_eq!(body["username"], CONSOLE_USER);
    assert_eq!(body["role"], "admin");
    body["token"].as_str().expect("console token").to_owned()
}
pub fn server_key_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{SERVER_KEY}:"))
    )
}
