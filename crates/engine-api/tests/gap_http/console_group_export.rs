use super::*;

pub async fn console_group_export(env: &Harness, console: &str) -> String {
    eprintln!("gap: console group export");
    let owner = device_player(&env.client, &env.base, "group").await;
    let name = format!("e2e group {}", letters_suffix());
    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&owner.token)
            .json(&json!({
                "name": name,
                "open": true,
                "max_count": 12,
                "description": "exported from the console",
                "metadata": "{\"tier\":\"gold\"}",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/group",
    )
    .await;
    let group_id = created["group"]["id"]
        .as_str()
        .expect("group id")
        .to_owned();

    let exported = json_body(
        env.client
            .get(format!("{}/v2/console/group/{group_id}/export", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/group/{id}/export",
    )
    .await;
    assert_eq!(exported["group"]["id"], group_id);
    assert_eq!(exported["group"]["name"], name);
    assert_eq!(
        json_string(&exported["group"]["metadata"]),
        json!({"tier": "gold"})
    );
    let members = exported["members"]
        .as_array()
        .expect("the export lists members");
    assert_eq!(members.len(), 1, "only the creator is a member");
    assert_eq!(members[0]["id"], owner.uid);
    assert_eq!(members[0]["state"], "superadmin");
    assert_eq!(members[0]["username"], owner.username);

    refusal(
        env.client
            .get(format!(
                "{}/v2/console/group/{}/export",
                env.base,
                Uuid::new_v4()
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "exporting a group that does not exist",
    )
    .await;

    expect(
        env.client
            .put(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .json(&json!({"description": "renamed by its leader", "open": false}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/group/{id} as the leader",
    )
    .await;
    let fetched = json_body(
        env.client
            .get(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/group/{id} after the update",
    )
    .await;
    assert_eq!(fetched["group"]["open"], false);
    assert_eq!(
        fetched["group"]["description"], "renamed by its leader",
        "the leader's edit is stored: {fetched}"
    );

    expect(
        env.client
            .delete(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/group/{id} as the leader",
    )
    .await;
    refusal(
        env.client
            .get(format!("{}/v2/group/{group_id}", env.base))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "reading a deleted group",
    )
    .await;
    group_id
}
