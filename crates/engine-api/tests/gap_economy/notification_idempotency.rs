use super::*;

pub async fn notification_idempotency(env: &Harness, console: &str) {
    eprintln!("gap: idempotent notification sends");
    let player = device_player(&env.client, &env.base, "notified").await;

    let send = |key: &str, subject: &str| {
        let client = env.client.clone();
        let base = env.base.clone();
        let token = console.to_owned();
        let uid = player.uid.clone();
        let key = key.to_owned();
        let subject = subject.to_owned();
        async move {
            client
                .post(format!("{base}/v2/console/notification"))
                .bearer_auth(&token)
                .header("Idempotency-Key", key)
                .json(&json!({"user_id": uid, "subject": subject, "content": {"v": 1}}))
                .send()
                .await
                .unwrap()
        }
    };

    json_body(
        send("notif-1", "Release notes").await,
        StatusCode::OK,
        "console sends a notification with a key",
    )
    .await;

    let body = send("notif-1", "Release notes").await;
    assert_eq!(body.status(), StatusCode::OK, "the replay answers 200");

    let rows = rows_matching(env, "notification", &player.uid).await;
    assert_eq!(
        rows.len(),
        1,
        "the replay did not send a second notification: {rows:?}"
    );

    let text = refusal(
        send("notif-1", "Something else").await,
        StatusCode::BAD_REQUEST,
        "reusing the key for a different subject",
    )
    .await;
    assert!(
        text.contains("already used for a different request"),
        "the reuse refusal names the collision: {text}"
    );
}
