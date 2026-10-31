use super::*;

pub async fn device_auth(client: &Client, base: &str, pool: &Datastore) {
    eprintln!("e2e: device authentication");
    let basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{SERVER_KEY}:"))
    );
    let device_id = format!("e2e-device-{}", Uuid::new_v4());
    let username = format!("e2e-player-{}", letters_suffix());
    let url = format!("{base}/v2/account/authenticate/device?username={username}&create=true");

    let created = json_body(
        client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, &basic)
            .json(&json!({ "id": device_id, "vars": { "platform": "e2e" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "device authenticate (create)",
    )
    .await;
    assert_eq!(created["created"], true);
    let uid = jwt_uid(&created["token"]);
    assert!(!created["refresh_token"].as_str().unwrap().is_empty());

    let resumed = json_body(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}"
            ))
            .header(reqwest::header::AUTHORIZATION, &basic)
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "device authenticate (resume)",
    )
    .await;
    assert!(resumed.get("created").is_none());
    assert_eq!(jwt_uid(&resumed["token"]), uid);
    assert_ne!(resumed["token"], created["token"]);

    refusal(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}"
            ))
            .header(
                reqwest::header::AUTHORIZATION,
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode("wrong-key:")
                ),
            )
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "device authenticate with a wrong server key",
    )
    .await;
    refusal(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}"
            ))
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "device authenticate without a server key",
    )
    .await;

    let stored: Value = scalar(
        pool,
        "SELECT VALUE user_id FROM type::record('user_device', $id) LIMIT 1",
        &[("id", json!(device_id))],
    )
    .await
    .expect("the device row was written");
    let stored_device = stored.as_str().unwrap_or_default().to_owned();
    assert_eq!(stored_device, uid);
    let stored_username: String = scalar(
        pool,
        "SELECT VALUE username FROM type::record('users', <uuid>$id) LIMIT 1",
        &[("id", json!(stored_device))],
    )
    .await
    .expect("the user row was written");
    assert_eq!(stored_username, username);
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
    let username = loop {
        let candidate = format!("e2e-{label}-{}", letters_suffix());
        if accepts_username(&candidate) {
            break candidate;
        }
    };
    let body = json_body(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}&create=true"
            ))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .expect("device authenticate for a tournament player"),
        StatusCode::OK,
        "device authenticate for a tournament player",
    )
    .await;
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
pub async fn define_tournament(client: &Client, base: &str, console: &str, body: Value) -> Value {
    json_body(
        client
            .post(format!("{base}/v2/console/tournament"))
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .expect("define a tournament"),
        StatusCode::OK,
        "define a tournament",
    )
    .await
}
