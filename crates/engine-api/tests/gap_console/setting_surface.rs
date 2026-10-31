use super::*;

pub async fn setting_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console settings");
    let name = format!("e2e.setting.{}", letters_suffix());

    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/console/setting", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "   ", "value": { "on": true } }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "writing a setting with no name",
        )
        .await,
        "A setting needs a name."
    );
    refusal(
        env.client
            .put(format!("{}/v2/console/setting", env.base))
            .bearer_auth(viewer)
            .json(&json!({ "name": name, "value": { "on": true } }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "writing a setting as a read-only operator",
    )
    .await;

    let written = json_body(
        env.client
            .put(format!("{}/v2/console/setting", env.base))
            .bearer_auth(console)
            .json(&json!({ "name": name, "value": { "rooms": 8, "beta": true } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/console/setting",
    )
    .await;
    assert_eq!(written["name"], name);
    assert_eq!(written["value"], json!({ "rooms": 8, "beta": true }));

    let listed = json_body(
        env.client
            .get(format!("{}/v2/console/setting", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/setting",
    )
    .await;
    assert_eq!(
        listed
            .as_array()
            .map(|rows| rows.iter().filter(|row| row["name"] == name).count())
            .unwrap_or_default(),
        1,
        "the write is listed once: {listed}"
    );

    expect(
        env.client
            .delete(format!("{}/v2/console/setting/{name}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/setting/{name}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/setting/{name}", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting a setting that is already gone",
        )
        .await,
        "Setting not found."
    );
}
