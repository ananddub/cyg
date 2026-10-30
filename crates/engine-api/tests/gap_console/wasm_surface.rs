use super::*;

pub async fn wasm_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console wasm module lifecycle");

    let wasm_game = create_game(env, console, "Gap WASM Game", "custom").await;
    let promoted_game = create_game(env, console, "Gap Promoted Game", "unity").await;

    let empty = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}/wasm",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/games/{gameId}/wasm on a fresh game",
    )
    .await;
    assert_eq!(empty.as_array().map(Vec::len), Some(0), "{empty}");

    refusal(
        env.client
            .post(format!(
                "{}/v2/console/games/{}/wasm/upload",
                env.base, wasm_game.id
            ))
            .bearer_auth(viewer)
            .multipart(module_form(ROOM_MODULE.to_vec()))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "uploading with a read-only operator",
    )
    .await;

    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/games/{}/wasm/upload",
                    env.base, wasm_game.id
                ))
                .bearer_auth(console)
                .multipart(module_form(b"not a wasm module at all".to_vec()))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "uploading bytes with no wasm magic",
        )
        .await,
        "Upload is not a WebAssembly binary: missing the \\0asm magic header."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/games/{}/wasm/upload",
                    env.base, wasm_game.id
                ))
                .bearer_auth(console)
                .multipart(module_form(Vec::new()))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "uploading an empty file part",
        )
        .await,
        "Upload needs a non-empty `file` field."
    );

    assert!(
        reloads(env).is_empty(),
        "a refused upload publishes no hot-swap request"
    );

    let detail_before = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}",
                env.base, promoted_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the unity game before its first upload",
    )
    .await;
    assert_eq!(detail_before["backend_mode"], "relay");
    let promoted = upload_module(
        env,
        console,
        &promoted_game.id,
        "/upload",
        module_form(ROOM_MODULE.to_vec()),
    )
    .await;
    assert_eq!(promoted["module"]["is_active"], true);
    let detail = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}",
                env.base, promoted_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the game an upload promoted",
    )
    .await;
    assert_eq!(detail["backend_mode"], "wasm");

    let first = upload_module(
        env,
        console,
        &wasm_game.id,
        "/upload",
        Form::new()
            .part(
                "file",
                Part::bytes(ROOM_MODULE.to_vec()).file_name("room_v1.wasm"),
            )
            .part("source_code", Part::text("fn on_tick() {}".to_owned())),
    )
    .await;
    let module = &first["module"];
    assert_eq!(module["version"], 1);
    assert_eq!(module["filename"], "room_v1.wasm");
    assert_eq!(module["is_active"], true);
    assert_eq!(module["has_source"], true, "the source listing is kept");
    assert_eq!(
        module["size_bytes"].as_i64(),
        Some(ROOM_MODULE.len() as i64)
    );
    assert_eq!(module["sha256"].as_str().map(str::len), Some(64));
    let digest = module_digest();
    assert_eq!(module["sha256"], digest, "the digest is of these bytes");

    let stored = row(
        env.db(),
        "SELECT * FROM room_modules \
         WHERE (game_id = <uuid>$game OR game_id = $game) AND version = $version LIMIT 1",
        &[("game", json!(wasm_game.id)), ("version", json!(1))],
    )
    .await
    .expect("the stored module");
    assert_eq!(stored["filename"], "room_v1.wasm");
    assert_eq!(stored["sha256"], digest);
    assert_eq!(stored["size_bytes"], json!(ROOM_MODULE.len() as i64));
    assert!(
        stored.get("binary_bytes").is_none(),
        "the payload is not a table column: {stored}"
    );
    let key = stored["blob_key"].as_str().expect("a blob key");
    assert_eq!(key, format!("wasm/{}/1.wasm", wasm_game.id));
    let bytes = env
        .blobs()
        .get(key)
        .await
        .expect("the blob reads")
        .expect("and is present");
    assert_eq!(bytes, ROOM_MODULE.to_vec(), "the binary round-trips byte for byte");
    assert_eq!(first["abi"]["valid"], true);
    assert_eq!(
        first["abi"]["exports_missing"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        0,
        "the ABI gate passed: {first}"
    );
    assert_eq!(first["abi"]["size_bytes"], json!(ROOM_MODULE.len()));

    let second = upload_module(
        env,
        console,
        &wasm_game.id,
        "",
        module_form(ROOM_MODULE.to_vec()),
    )
    .await;
    assert_eq!(second["module"]["version"], 2);
    assert_eq!(second["module"]["is_active"], true);
    assert_eq!(
        second["module"]["has_source"], false,
        "no source part means no source column"
    );

    let drained = reloads(env);
    let promoted_request = drained
        .iter()
        .find(|request| request.game_id == promoted_game.id)
        .expect("the promoted upload published a hot-swap request");
    assert_eq!(promoted_request.kind, ReloadKind::Upload);
    assert_eq!(promoted_request.version, 1);
    assert_eq!(promoted_request.sha256, digest);
    assert_eq!(
        promoted_request.bytes,
        ROOM_MODULE.to_vec(),
        "the sim plane is handed the module itself, not an identity it cannot resolve"
    );
    let published: Vec<_> = drained
        .into_iter()
        .filter(|request| request.game_id == wasm_game.id)
        .collect();
    assert_eq!(published.len(), 2, "one request per upload: {published:?}");
    for (index, request) in published.iter().enumerate() {
        assert_eq!(request.kind, ReloadKind::Upload, "{request:?}");
        assert_eq!(request.version, index as i32 + 1, "{request:?}");
        assert_eq!(request.game_id, wasm_game.id, "{request:?}");
        assert_eq!(request.game_key, wasm_game.key, "{request:?}");
        assert_eq!(request.sha256, digest, "{request:?}");
        assert_eq!(request.bytes, ROOM_MODULE.to_vec(), "{request:?}");
    }

    let listing = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}/wasm",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list the versions",
    )
    .await;
    let rows = listing.as_array().expect("an array of modules").clone();
    assert_eq!(rows.len(), 2, "both versions are stored, newest first");
    assert_eq!(rows[0]["version"], 2);
    assert_eq!(rows[1]["version"], 1);
    assert_eq!(
        rows.iter().filter(|m| m["is_active"] == true).count(),
        1,
        "exactly one version is active"
    );

    let detail = json_body(
        env.client
            .get(format!("{}/v2/console/games/{}", env.base, wasm_game.id))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "game detail after two uploads",
    )
    .await;
    assert_eq!(detail["module_count"], 2);
    assert_eq!(detail["active_module_version"], 2);

    let rolled_back = json_body(
        env.client
            .post(format!(
                "{}/v2/console/games/{}/wasm/rollback",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .json(&json!({ "version": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/games/{gameId}/wasm/rollback",
    )
    .await;
    assert_eq!(rolled_back["version"], 1);
    assert_eq!(rolled_back["is_active"], true);
    let rolled_back_request = reloads(env).pop().expect("the rollback published");
    assert_eq!(rolled_back_request.kind, ReloadKind::Rollback);
    assert_eq!(rolled_back_request.version, 1);
    assert_eq!(rolled_back_request.sha256, digest);
    assert_eq!(
        rolled_back_request.bytes,
        ROOM_MODULE.to_vec(),
        "a rollback hands the older version's bytes to the sim as well"
    );
    let detail = json_body(
        env.client
            .get(format!("{}/v2/console/games/{}", env.base, wasm_game.id))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "game detail after the rollback",
    )
    .await;
    assert_eq!(detail["active_module_version"], 1);

    let activated = json_body(
        env.client
            .post(format!(
                "{}/v2/console/games/{}/wasm/activate",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .json(&json!({ "version": 2 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/games/{gameId}/wasm/activate",
    )
    .await;
    assert_eq!(activated["version"], 2);
    let activated_request = reloads(env).pop().expect("the activation published");
    assert_eq!(activated_request.kind, ReloadKind::Upload);
    assert_eq!(activated_request.version, 2);
    assert_eq!(
        activated_request.bytes,
        ROOM_MODULE.to_vec(),
        "activation re-reads the stored bytes for the sim"
    );

    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/games/{}/wasm/2",
                    env.base, wasm_game.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "deleting the active module",
        )
        .await,
        "Cannot delete the active module; activate another version first."
    );
    assert!(
        reloads(env).is_empty(),
        "a refused delete publishes no hot-swap request"
    );
    json_body(
        env.client
            .post(format!(
                "{}/v2/console/games/{}/wasm/activate",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .json(&json!({ "version": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "activate version 1 so 2 can go",
    )
    .await;
    let swapped_back = reloads(env);
    assert_eq!(swapped_back.len(), 1, "an activation publishes once");
    assert_eq!(swapped_back[0].version, 1);
    assert_eq!(swapped_back[0].kind, ReloadKind::Upload);
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/games/{}/wasm/2",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/games/{gameId}/wasm/{version}",
    )
    .await;
    assert!(
        reloads(env).is_empty(),
        "dropping an inactive version tells the sim plane nothing"
    );
    let listing = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}/wasm",
                env.base, wasm_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list after the delete",
    )
    .await;
    assert_eq!(listing.as_array().map(Vec::len), Some(1));

    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/games/{}/wasm/not-a-number",
                    env.base, wasm_game.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "deleting by a version that is not a number",
        )
        .await,
        "Invalid module version."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/games/{}/wasm/activate",
                    env.base, wasm_game.id
                ))
                .bearer_auth(console)
                .json(&json!({ "version": 99 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "activating a version that was never uploaded",
        )
        .await,
        "Module version 99 not found."
    );
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/games/{}/wasm/98",
                    env.base, wasm_game.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting a version that was never uploaded",
        )
        .await,
        "Module version 98 not found."
    );
    assert_eq!(
        refusal(
            env.client
                .get(format!("{}/v2/console/games/bogus/wasm", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a malformed game id",
        )
        .await,
        "Invalid game id."
    );
    let stranger = Uuid::new_v4().to_string();
    for (label, call) in [
        (
            "listing modules of a game that does not exist",
            env.client
                .get(format!("{}/v2/console/games/{stranger}/wasm", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
        ),
        (
            "deleting a module of a game that does not exist",
            env.client
                .delete(format!("{}/v2/console/games/{stranger}/wasm/1", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
        ),
    ] {
        assert_eq!(
            refusal(call, StatusCode::NOT_FOUND, label,).await,
            "Game not found.",
            "{label}"
        );
    }

    let asset = env
        .client
        .get(format!(
            "{}/v2/console/games/{}/unity-settings",
            env.base, promoted_game.id
        ))
        .bearer_auth(console)
        .send()
        .await
        .unwrap();
    let typed = asset
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let disposition = asset
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = asset.text().await.expect("the asset body");
    expect_text(
        &typed,
        "application/octet-stream",
        "the Unity asset downloads as bytes",
    );
    assert!(
        disposition.contains("CYGServerSettings.asset"),
        "named for the Unity editor: {disposition}"
    );
    assert!(
        body.contains("serverHttpUrl") && body.contains("matchmakeMethod"),
        "the asset is the editor's own document: {body}"
    );

    let settings = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}/unity-settings?format=json",
                env.base, promoted_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/games/{gameId}/unity-settings?format=json",
    )
    .await;
    assert_eq!(settings["gameKey"], promoted_game.key);
    assert_eq!(settings["clientKey"], promoted_game.client_key);
    assert_eq!(settings["backendMode"], "wasm");
    assert!(
        settings.get("apiKey").is_none(),
        "the secret stays out of the default document: {settings}"
    );

    let revealed = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}/unity-settings?format=json&reveal=true",
                env.base, promoted_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unity settings with the secret revealed",
    )
    .await;
    assert!(
        revealed["apiKey"]
            .as_str()
            .unwrap_or_default()
            .starts_with("cyg_sec_"),
        "reveal hands over the tenant secret: {revealed}"
    );

    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/v2/console/games/{}/unity-settings?format=xml",
                    env.base, promoted_game.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an unknown unity-settings format",
        )
        .await,
        "Unknown unity-settings format 'xml'; expected asset or json."
    );

    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/console/games/{}", env.base, wasm_game.id))
                .bearer_auth(console)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a patch that changes nothing",
        )
        .await,
        "A game update has to change at least one field."
    );
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/console/games/{}", env.base, wasm_game.id))
                .bearer_auth(console)
                .json(&json!({ "status": "maintenance" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an unknown game status",
        )
        .await,
        "Unknown game status 'maintenance'; expected one of active, suspended, archived."
    );
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/console/games/{}", env.base, wasm_game.id))
                .bearer_auth(console)
                .json(&json!({ "config": 64 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a config that is not an object",
        )
        .await,
        "A game config must be a JSON object."
    );
    let renamed = json_body(
        env.client
            .put(format!("{}/v2/console/games/{}", env.base, wasm_game.id))
            .bearer_auth(console)
            .json(&json!({
                "name": "Gap WASM Renamed",
                "status": "suspended",
                "config": { "tick_rate": 64, "max_clients": 8 },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/console/games/{gameId}",
    )
    .await;
    assert_eq!(renamed["name"], "Gap WASM Renamed");
    assert_eq!(renamed["status"], "suspended");
    assert_eq!(renamed["config"]["tick_rate"], 64);
    assert_eq!(renamed["backend_mode"], "wasm");

    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/games/{}", env.base, promoted_game.id))
                .bearer_auth(viewer)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "archiving a game as a read-only operator",
        )
        .await,
        "This console action requires the admin role."
    );
    let archived = json_body(
        env.client
            .delete(format!("{}/v2/console/games/{}", env.base, promoted_game.id))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/games/{gameId}",
    )
    .await;
    assert_eq!(archived["archived"], true);
}
