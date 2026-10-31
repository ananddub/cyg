use super::*;

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
pub fn letters_suffix() -> String {
    let mut value = Uuid::new_v4().as_u128();
    let mut out = String::new();
    while out.len() < 8 {
        out.push((b'a' + (value % 26) as u8) as char);
        value /= 26;
    }
    out
}
pub fn accepts_username(candidate: &str) -> bool {
    use rustrict::{Censor, Type};
    !Censor::from_str(candidate)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .analyze()
        .is(Type::INAPPROPRIATE)
}
pub fn jwt_uid(token: &Value) -> String {
    let token = token.as_str().expect("session token");
    let payload = token.split('.').nth(1).expect("JWT payload segment");
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("base64url JWT payload");
    let claims: Value = serde_json::from_slice(&decoded).expect("JWT claims JSON");
    claims["uid"].as_str().expect("uid claim").to_owned()
}
pub struct Player {
    pub token: String,
    pub refresh_token: String,
    pub uid: String,
    pub username: String,
    pub device_id: String,
}
pub async fn device_player(client: &Client, base: &str, label: &str) -> Player {
    let device_id = format!("e2e-{label}-{}", Uuid::new_v4());
    let username = (0..50)
        .map(|_| format!("e2e-{label}-{}", letters_suffix()))
        .find(|candidate| accepts_username(candidate))
        .unwrap_or_else(|| panic!("`e2e-{label}-<suffix>` never yields a clean username"));
    let body = json_body(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}&create=true"
            ))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "id": device_id, "vars": { "platform": "e2e" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "device authenticate",
    )
    .await;
    assert_eq!(body["created"], true, "{label}: device account created");
    Player {
        token: body["token"].as_str().expect("session token").to_owned(),
        refresh_token: body["refresh_token"]
            .as_str()
            .expect("refresh token")
            .to_owned(),
        uid: jwt_uid(&body["token"]),
        username,
        device_id,
    }
}
pub fn json_string(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
        other => other.clone(),
    }
}
pub fn bind(params: &[(&str, Value)]) -> std::collections::BTreeMap<String, Value> {
    params
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}
pub async fn scalar<T: serde::de::DeserializeOwned>(
    db: &Datastore,
    surql: &str,
    params: &[(&str, Value)],
) -> Option<T> {
    let bound = bind(params);
    db.query_params::<T>(surql, &bound)
        .await
        .unwrap_or_else(|err| panic!("{surql}: {err}"))
        .into_iter()
        .next()
}
pub async fn row(
    db: &Datastore,
    surql: &str,
    params: &[(&str, Value)],
) -> Option<Value> {
    scalar::<Value>(db, surql, params).await
}
pub async fn rows(
    db: &Datastore,
    surql: &str,
    params: &[(&str, Value)],
) -> Vec<Value> {
    let bound = bind(params);
    db.query_params::<Value>(surql, &bound)
        .await
        .unwrap_or_else(|err| panic!("{surql}: {err}"))
}
pub async fn count(
    db: &Datastore,
    table: &str,
    filter: Option<&str>,
    params: &[(&str, Value)],
) -> i64 {
    let surql = match filter {
        Some(filter) => format!("RETURN array::len((SELECT id FROM {table} WHERE {filter}))"),
        None => format!("RETURN array::len((SELECT id FROM {table}))"),
    };
    scalar::<i64>(db, &surql, params).await.unwrap_or(0)
}
pub async fn exec(db: &Datastore, surql: &str, params: &[(&str, Value)]) {
    let bound = bind(params);
    db.exec_params(surql, &bound)
        .await
        .unwrap_or_else(|err| panic!("{surql}: {err}"));
}
