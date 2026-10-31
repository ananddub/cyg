use super::*;

pub async fn api_get(client: &Client, url: String, token: &str, what: &str) -> Value {
    json_body(
        client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}
pub async fn api_post(client: &Client, url: String, token: &str, body: Value, what: &str) -> Value {
    json_body(
        client
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}
pub async fn api_post_empty(client: &Client, url: String, token: &str, what: &str) -> Value {
    json_body(
        client
            .post(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}
pub async fn api_put(client: &Client, url: String, token: &str, body: Value, what: &str) -> Value {
    json_body(
        client
            .put(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}
pub async fn api_delete(client: &Client, url: String, token: &str, what: &str) -> Value {
    json_body(
        client
            .delete(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}
pub async fn api_delete_body(
    client: &Client,
    url: String,
    token: &str,
    body: Value,
    what: &str,
) -> Value {
    json_body(
        client
            .delete(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}
pub fn with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse(base).expect("a valid base URL");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    url.to_string()
}
pub fn uuid_of(player: &Player) -> Uuid {
    Uuid::parse_str(&player.uid).expect("the session's uid claim is a UUID")
}
pub fn json_string(value: &Value) -> Value {
    serde_json::from_str(value.as_str().expect("a JSON string member"))
        .unwrap_or_else(|err| panic!("member is not JSON ({err}): {value}"))
}
pub async fn edge_count(pool: &Datastore, uid: Uuid) -> i32 {
    scalar::<i32>(
        pool,
        "SELECT VALUE edge_count FROM type::record('users', <uuid>$id) LIMIT 1",
        &[("id", json!(uid.to_string()))],
    )
    .await
    .expect("users.edge_count")
}
pub async fn friend_edge_state(pool: &Datastore, from: Uuid, to: Uuid) -> Option<i16> {
    scalar::<i16>(
        pool,
        "SELECT VALUE state FROM user_edge
         WHERE (source_id = <uuid>$from OR source_id = $from)
           AND (destination_id = <uuid>$to OR destination_id = $to) LIMIT 1",
        &[("from", json!(from.to_string())), ("to", json!(to.to_string()))],
    )
    .await
}
pub async fn group_edge_state(pool: &Datastore, uid: Uuid, group: Uuid) -> Option<i16> {
    scalar::<i16>(
        pool,
        "SELECT VALUE state FROM group_edge
         WHERE (source_id = <uuid>$from OR source_id = $from)
           AND (destination_id = <uuid>$to OR destination_id = $to) LIMIT 1",
        &[("from", json!(uid.to_string())), ("to", json!(group.to_string()))],
    )
    .await
}
pub async fn group_count(pool: &Datastore, group: Uuid) -> i32 {
    scalar::<i32>(
        pool,
        "SELECT VALUE edge_count FROM type::record('groups', <uuid>$id) LIMIT 1",
        &[("id", json!(group.to_string()))],
    )
    .await
    .expect("groups.edge_count")
}
