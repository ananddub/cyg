use super::*;

pub async fn wasm_lifecycle(
    client: &Client,
    base: &str,
    console: &str,
    relay_game: &str,
    wasm_game: &str,
) {
    eprintln!("e2e: wasm module lifecycle");
    let wasm = module_bytes();
    let upload_url = |path: &str| format!("{base}/v2/console/games/{wasm_game}/wasm{path}");

    let form = Form::new()
        .part(
            "file",
            Part::bytes(wasm.clone()).file_name("room_logic.wasm"),
        )
        .text("source_code", "// e2e room module");
    let first = json_body(
        client
            .post(upload_url("/upload"))
            .bearer_auth(console)
            .multipart(form)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "upload the first module",
    )
    .await;
    assert_eq!(first["abi"]["valid"], true);
    assert_eq!(first["abi"]["wasm_magic_ok"], true);
    assert_eq!(first["abi"]["exports_missing"].as_array().unwrap().len(), 0);
    assert_eq!(
        first["abi"]["exports_found"].as_array().unwrap().len(),
        engine_api::services::wasm::REQUIRED_EXPORTS.len()
    );
    assert_eq!(first["module"]["version"], 1);
    assert_eq!(first["module"]["is_active"], true);
    assert_eq!(first["module"]["filename"], "room_logic.wasm");
    assert_eq!(first["module"]["has_source"], true);
    assert_eq!(first["module"]["size_bytes"], wasm.len());
    assert_eq!(first["module"]["sha256"].as_str().unwrap().len(), 64);

    let broken = Form::new().part(
        "file",
        Part::bytes(b"definitely not wasm".to_vec()).file_name("broken.wasm"),
    );
    let message = refusal(
        client
            .post(upload_url("/upload"))
            .bearer_auth(console)
            .multipart(broken)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "upload non-wasm bytes",
    )
    .await;
    assert!(message.contains("WebAssembly"), "{message}");

    let listing = json_body(
        client
            .get(upload_url(""))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list modules",
    )
    .await;
    assert_eq!(listing.as_array().unwrap().len(), 1);

    let second_form = Form::new().part(
        "file",
        Part::bytes(wasm.clone()).file_name("room_logic_v2.wasm"),
    );
    let second = json_body(
        client
            .post(upload_url(""))
            .bearer_auth(console)
            .multipart(second_form)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "upload the second module",
    )
    .await;
    assert_eq!(second["module"]["version"], 2);
    assert_eq!(second["module"]["is_active"], true);
    assert_eq!(second["module"]["has_source"], false);

    let listing = json_body(
        client
            .get(upload_url(""))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list modules after upload",
    )
    .await;
    let listing = listing.as_array().unwrap();
    assert_eq!(listing.len(), 2);
    assert_eq!(listing[0]["version"], 2);
    assert_eq!(listing[1]["version"], 1);
    assert_eq!(listing[0]["is_active"], true);
    assert_eq!(listing[1]["is_active"], false);

    let activated = json_body(
        client
            .post(upload_url("/activate"))
            .bearer_auth(console)
            .json(&json!({ "version": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "activate version 1",
    )
    .await;
    assert_eq!(activated["version"], 1);
    assert_eq!(activated["is_active"], true);

    let rolled_back = json_body(
        client
            .post(upload_url("/rollback"))
            .bearer_auth(console)
            .json(&json!({ "version": 2 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "roll forward via rollback",
    )
    .await;
    assert_eq!(rolled_back["version"], 2);
    assert_eq!(rolled_back["is_active"], true);

    let detail = json_body(
        client
            .get(format!("{base}/v2/console/games/{wasm_game}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "game detail after activation",
    )
    .await;
    assert_eq!(detail["active_module_version"], 2);
    assert_eq!(detail["module_count"], 2);

    let message = refusal(
        client
            .delete(upload_url("/2"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete the active module",
    )
    .await;
    assert!(message.contains("active module"), "{message}");

    let body = expect(
        client
            .delete(upload_url("/1"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete an inactive module",
    )
    .await;
    assert!(body.is_empty());

    let listing = json_body(
        client
            .get(upload_url(""))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list modules after delete",
    )
    .await;
    let listing = listing.as_array().unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0]["version"], 2);

    refusal(
        client
            .post(upload_url("/activate"))
            .bearer_auth(console)
            .json(&json!({ "version": 9 }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "activate a missing version",
    )
    .await;

    let relay_form = Form::new().part(
        "file",
        Part::bytes(wasm.clone()).file_name("room_logic.wasm"),
    );
    let promoted = json_body(
        client
            .post(format!("{base}/v2/console/games/{relay_game}/wasm/upload"))
            .bearer_auth(console)
            .multipart(relay_form)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "uploading to a relay game promotes it",
    )
    .await;
    assert_eq!(promoted["module"]["is_active"], true);
    let detail = json_body(
        client
            .get(format!("{base}/v2/console/games/{relay_game}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the promoted game",
    )
    .await;
    assert_eq!(detail["backend_mode"], "wasm");

    expect(
        client
            .delete(format!("{base}/v2/console/games/not-a-uuid/wasm/1"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete from a malformed game id",
    )
    .await;
    expect(
        client
            .delete(format!("{base}/v2/console/games/{}/wasm/1", Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete from an unknown game",
    )
    .await;
}
