use super::*;

pub async fn console_auth(client: &Client, base: &str, pool: &Datastore) -> String {
    eprintln!("e2e: console auth");
    let wrong = refusal(
        client
            .post(format!("{base}/v2/console/authenticate"))
            .json(&json!({ "username": CONSOLE_USER, "password": "wrong" }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console sign-in with a wrong password",
    )
    .await;
    assert_eq!(wrong, "Invalid credentials.");

    expect(
        client
            .get(format!("{base}/v2/console/games"))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console list without a token",
    )
    .await;

    let token = console_login(client, base).await;
    let list = json_body(
        client
            .get(format!("{base}/v2/console/games"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "console list with a token",
    )
    .await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["games"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["games"][0]["name"], GAME_NAME);

    let logins = count(
        pool,
        "console_audit_log",
        Some("action = 'authenticate.login'"),
        &[],
    )
    .await;
    assert!(logins >= 1, "the console sign-in must leave an audit row");

    expect(
        client
            .post(format!("{base}/v2/console/authenticate/logout"))
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "console logout",
    )
    .await;
    expect(
        client
            .get(format!("{base}/v2/console/games"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console list after logout",
    )
    .await;

    console_login(client, base).await
}
