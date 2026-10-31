use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn console_module_iap_settings_and_storage_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    let viewer_name = format!("e2e-cview-{}", letters_suffix());
    let viewer_password = "e2e-console-viewer";
    json_body(
        env.client
            .post(format!("{}/v2/console/user", env.base))
            .bearer_auth(&console)
            .json(&json!({
                "username": viewer_name,
                "email": format!("{viewer_name}@example.com"),
                "password": viewer_password,
                "role": "readonly",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a read-only operator",
    )
    .await;
    let viewer = console_login_as(&env, &viewer_name, viewer_password, json!({})).await;

    wasm_surface(&env, &console, &viewer).await;
    iap_surface(&env, &console, &viewer).await;
    setting_surface(&env, &console, &viewer).await;
    storage_surface(&env, &console, &viewer).await;
    group_and_board_surface(&env, &console, &viewer).await;
    operator_surface(&env, &console, &viewer).await;

    let trail = json_body(
        env.client
            .get(format!("{}/v2/console/audit/log?limit=100", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the audit trail of this suite",
    )
    .await;
    let actions: Vec<String> = trail["logs"]
        .as_array()
        .or(trail.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["action"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for wanted in [
        "iap.validate",
        "setting.update",
        "setting.delete",
        "storage.object.delete",
        "storage.collection.delete",
        "account.ban",
        "account.unban",
        "account.note.create",
        "account.note.delete",
        "acl.template.create",
        "acl.template.update",
        "acl.template.delete",
        "console_user.create",
        "console_user.password",
        "console_user.disable",
        "console_user.enable",
        "leaderboard.record.delete",
        "leaderboard.delete",
    ] {
        assert!(
            actions.iter().any(|action| action == wanted),
            "the trail records {wanted}: {actions:?}"
        );
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!(
        "gap: console module, iap, setting, storage, roster, board and operator surfaces passed"
    );
}
