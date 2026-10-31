use super::*;

pub fn secret_code(secret: &str) -> String {
    totp::code_at(secret, chrono::Utc::now().timestamp()).expect("a TOTP for the live window")
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
