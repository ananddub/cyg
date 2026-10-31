use super::*;

pub const ROOM_MODULE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/engine-net/examples/e2e/room_logic.wasm"
));
pub fn module_digest() -> String {
    hex::encode(Sha256::digest(ROOM_MODULE))
}
pub fn reloads(env: &Harness) -> Vec<WasmReload> {
    let mut published = Vec::new();
    while let Ok(reload) = env.bus.reloads_rx.try_recv() {
        published.push(reload);
    }
    published
}
pub struct Game {
    pub id: String,
    pub key: String,
    pub client_key: String,
}
pub async fn create_game(env: &Harness, console: &str, name: &str, engine: &str) -> Game {
    let created = json_body(
        env.client
            .post(format!("{}/v2/console/games", env.base))
            .bearer_auth(console)
            .json(&json!({ "name": name, "engine_type": engine }))
            .send()
            .await
            .expect("create a game"),
        StatusCode::OK,
        &format!("create a {engine} game"),
    )
    .await;
    Game {
        id: created["id"].as_str().expect("game id").to_owned(),
        key: created["game_key"].as_str().expect("game key").to_owned(),
        client_key: created["client_key"]
            .as_str()
            .expect("client key")
            .to_owned(),
    }
}
pub async fn upload_module(env: &Harness, token: &str, game: &str, path: &str, body: Form) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/console/games/{game}/wasm{path}", env.base))
            .bearer_auth(token)
            .multipart(body)
            .send()
            .await
            .expect("upload a module"),
        StatusCode::OK,
        "upload a room module",
    )
    .await
}
pub fn module_form(bytes: Vec<u8>) -> Form {
    Form::new().part("file", Part::bytes(bytes).file_name("room_logic.wasm"))
}
pub async fn console_login_as(env: &Harness, username: &str, password: &str, extra: Value) -> String {
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
pub fn expect_text(actual: &str, want: &str, what: &str) {
    assert!(
        actual.starts_with(want),
        "{what}: expected {want}, got {actual}"
    );
}
