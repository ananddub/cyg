use super::*;

pub async fn put_object(env: &Harness, token: &str, collection: &str, key: &str, value: Value) {
    json_body(
        env.client
            .put(format!("{}/v2/storage", env.base))
            .bearer_auth(token)
            .json(&json!({
                "objects": [{
                    "collection": collection,
                    "key": key,
                    "value": value.to_string(),
                }]
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/storage",
    )
    .await;
}
pub async fn seed_board(
    env: &Harness,
    id: &str,
    category: i16,
    duration: i32,
    sort_order: i16,
    authoritative: bool,
) {
    let now = chrono::Utc::now();
    exec(
        env.db(),
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
                    "end_time": engine_api::db::time::EPOCH,
                    "join_required": false,
                    "max_size": 100_000_000,
                    "max_num_score": 1_000_000,
                    "title": id,
                    "size": 0,
                    "start_time": engine_api::db::time::encode(now),
                    "enable_ranks": true,
                    "game_id": Value::Null,
                }),
            ),
        ],
    )
    .await;
}
pub async fn member_state(env: &Harness, token: &str, group_id: &str, uid: &str) -> Option<i16> {
    let page = json_body(
        env.client
            .get(format!("{}/v2/group/{group_id}/user?limit=100", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{groupId}/user (state lookup)",
    )
    .await;
    page["group_users"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["user"]["id"] == uid)
        .map(|row| row["state"].as_i64().expect("member state") as i16)
}
pub async fn member_in_state(
    env: &Harness,
    token: &str,
    group_id: &str,
    uid: &str,
    state: i16,
) -> bool {
    let page = json_body(
        env.client
            .get(format!(
                "{}/v2/group/{group_id}/user?state={state}&limit=100",
                env.base
            ))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{groupId}/user?state= (bucket lookup)",
    )
    .await;
    page["group_users"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|row| row["user"]["id"] == uid)
}
pub async fn members_call(env: &Harness, token: &str, path: &str, what: &str) {
    expect(
        env.client
            .post(format!("{}{}", env.base, path))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        what,
    )
    .await;
}
