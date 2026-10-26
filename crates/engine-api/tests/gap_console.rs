//! The console surface no earlier suite drives end to end: a game's WASM module
//! lifecycle (upload → list → activate/rollback → delete) and its Unity settings,
//! vendor IAP validation and maintenance, server settings, the storage browser,
//! group membership and board records.
//!
//! Every call goes through the real router against a real Postgres 16 in a
//! Testcontainer, and every assertion reads the reply the way the dashboard does:
//! status, message wording and payload keys.

use std::time::Duration;

use engine_bus::{ReloadKind, WasmReload};
use reqwest::multipart::{Form, Part};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness;

use harness::*;

/// A guest module that satisfies the room ABI: the eight exports the host binds.
/// Reused for the uploads because a stored version has to be loadable bytes, not
/// a placeholder — the ABI gate refuses anything else.
const ROOM_MODULE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/engine-net/examples/e2e/room_logic.wasm"
));

/// `room_modules.sha256`'s contract: lowercase hex of the binary's digest. The
/// hot-reload reader compares this against whatever it pulls from the database,
/// so the column has to be the real digest of the uploaded bytes.
fn module_digest() -> String {
    hex::encode(Sha256::digest(ROOM_MODULE))
}

/// The hot-swap requests the control plane published since the last call, in
/// order. Nothing in this process drains `reloads_rx` but the test, so the count
/// is exact: every refusal that stops before the write publishes nothing.
fn reloads(env: &Harness) -> Vec<WasmReload> {
    let mut published = Vec::new();
    while let Ok(reload) = env.bus.reloads_rx.try_recv() {
        published.push(reload);
    }
    published
}

/// A game registered through the console, as the routes address it.
struct Game {
    id: String,
    key: String,
    client_key: String,
}

async fn create_game(env: &Harness, console: &str, name: &str, engine: &str) -> Game {
    let created = json_body(
        env.client
            .post(format!("{}/v2/console/games", env.base))
            .bearer_auth(console)
            .json(&json!({ "name": name, "engine_type": engine }))
            .send()
            .await
            .expect("create a game"),
        StatusCode::OK,
        &format!("create a {engine} game"),
    )
    .await;
    Game {
        id: created["id"].as_str().expect("game id").to_owned(),
        key: created["game_key"].as_str().expect("game key").to_owned(),
        client_key: created["client_key"]
            .as_str()
            .expect("client key")
            .to_owned(),
    }
}

async fn upload_module(env: &Harness, token: &str, game: &str, path: &str, body: Form) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/console/games/{game}/wasm{path}", env.base))
            .bearer_auth(token)
            .multipart(body)
            .send()
            .await
            .expect("upload a module"),
        StatusCode::OK,
        "upload a room module",
    )
    .await
}

fn module_form(bytes: Vec<u8>) -> Form {
    Form::new().part("file", Part::bytes(bytes).file_name("room_logic.wasm"))
}

/// Signs in as an operator this suite created itself.
async fn console_login_as(env: &Harness, username: &str, password: &str, extra: Value) -> String {
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

/// One player storage write, through the route the game itself uses.
async fn put_object(env: &Harness, token: &str, collection: &str, key: &str, value: Value) {
    json_body(
        env.client
            .put(format!("{}/v2/storage", env.base))
            .bearer_auth(token)
            .json(&json!({
                "objects": [{
                    "collection": collection,
                    "key": key,
                    "value": value.to_string(),
                }]
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/storage",
    )
    .await;
}

/// A content type the dashboard reads off the wire, checked by prefix because the
/// server may append a charset or a boundary.
fn expect_text(actual: &str, want: &str, what: &str) {
    assert!(
        actual.starts_with(want),
        "{what}: expected {want}, got {actual}"
    );
}

/// The module store of one game: upload both spellings, the ABI gate, the
/// active-version flip, and the delete rules.
async fn wasm_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console wasm module lifecycle");

    let wasm_game = create_game(env, console, "Gap WASM Game", "custom").await;
    let relay_game = create_game(env, console, "Gap Relay Game", "unity").await;

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

    // A read-only operator may list but never upload.
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
                    env.base, relay_game.id
                ))
                .bearer_auth(console)
                .multipart(module_form(ROOM_MODULE.to_vec()))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "uploading to a relay game",
        )
        .await,
        "Game is configured for Unity C# Relay. WASM upload is only applicable for Web/Custom Authoritative games."
    );

    // Bytes that are not a module, and a part that is empty.
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

    // Every refusal above stopped before the write, so the sim was never asked
    // to swap onto bytes it could not load.
    assert!(
        reloads(env).is_empty(),
        "a refused upload publishes no hot-swap request"
    );

    // The real module, at the `/upload` spelling, with its source alongside.
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

    // What a hot-reload would fetch is the upload itself: same name, same bytes,
    // same digest, read straight out of `room_modules`.
    let stored: (String, Vec<u8>, String) = sqlx::query_as(
        "SELECT filename, binary_bytes, sha256 FROM room_modules \
         WHERE game_id = $1 AND version = $2",
    )
    .bind(Uuid::parse_str(&wasm_game.id).expect("a game id"))
    .bind(1i32)
    .fetch_one(&env.pool)
    .await
    .expect("the stored module");
    assert_eq!(stored.0, "room_v1.wasm");
    assert_eq!(stored.2, digest);
    assert_eq!(
        stored.1,
        ROOM_MODULE.to_vec(),
        "the binary round-trips byte for byte"
    );
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

    // The same upload at the collection path: one shared implementation.
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

    // Each upload asked the sim plane to hot-swap onto the version it just made
    // active — that message is the only signal a running room ever gets.
    let published = reloads(env);
    assert_eq!(published.len(), 2, "one request per upload: {published:?}");
    for (index, request) in published.iter().enumerate() {
        assert_eq!(request.kind, ReloadKind::Upload, "{request:?}");
        assert_eq!(request.version, index as i32 + 1, "{request:?}");
        assert_eq!(request.game_id, wasm_game.id, "{request:?}");
        assert_eq!(request.game_key, wasm_game.key, "{request:?}");
        assert_eq!(request.sha256, digest, "{request:?}");
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

    // The game detail reads the same registry.
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

    // activate/rollback are the same flip, spelled two ways.
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

    // The active version cannot be deleted; an inactive one can.
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

    // Its own refusals.
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

    // The Unity settings document: the editor asset is the default spelling and
    // the camelCase connection JSON is what `?format=json` answers with.
    let asset = env
        .client
        .get(format!(
            "{}/v2/console/games/{}/unity-settings",
            env.base, relay_game.id
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
                env.base, relay_game.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/games/{gameId}/unity-settings?format=json",
    )
    .await;
    assert_eq!(settings["gameKey"], relay_game.key);
    assert_eq!(settings["clientKey"], relay_game.client_key);
    assert_eq!(settings["backendMode"], "relay");
    assert!(
        settings.get("apiKey").is_none(),
        "the secret stays out of the default document: {settings}"
    );

    let revealed = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{}/unity-settings?format=json&reveal=true",
                env.base, relay_game.id
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
                    env.base, relay_game.id
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

    // PUT /v2/console/games/{gameId}: the patch rules no earlier suite drives.
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
    // A status outside the three lifecycles is named back, so an operator typing
    // "deleted" learns the vocabulary instead of guessing it again.
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
    // The mode is derived from the engine, never from the body: a patch that names
    // no engine leaves it alone.
    assert_eq!(renamed["backend_mode"], "wasm");

    // The archive is admin-only: the read-only operator cannot even ask.
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/games/{}", env.base, relay_game.id))
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
            .delete(format!("{}/v2/console/games/{}", env.base, relay_game.id))
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

/// The vendor IAP surface: an operator validates a credential, the ledger lists
/// what was written, and both the product rows and the stored vendor config can
/// be cleared.
async fn iap_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console iap validate, ledger and reset");
    let player = device_player(&env.client, &env.base, "iap").await;

    // Nothing is written until the operator asks for it.
    let dry = json_body(
        env.client
            .post(format!("{}/v2/console/iap/apple/validate", env.base))
            .bearer_auth(console)
            .json(&json!({ "receipt": "e2e-apple-receipt", "product_id": "gem.100", "persist": false }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "an unpersisted purchase validation",
    )
    .await;
    let parsed = &dry["validated_purchases"][0];
    assert_eq!(parsed["product_id"], "gem.100");
    assert_eq!(
        parsed["transaction_id"].as_str().map(str::len),
        Some(32),
        "the credential is hashed into the transaction id: {dry}"
    );
    assert!(
        dry["validated_purchases"][0]
            .get("provider_notification")
            .is_none(),
        "a plain purchase reply carries no notification"
    );
    assert_eq!(
        json_body(
            env.client
                .get(format!("{}/v2/console/purchase", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "the ledger stays empty after a dry validation",
        )
        .await
        .as_array()
        .map(Vec::len)
        .unwrap_or_default(),
        0
    );

    // Persisting needs an account to own the row.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/apple/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "e2e-apple-receipt" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "persisting with no user id",
        )
        .await,
        "Persisting a validation needs a user id."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/apple/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "x", "user_id": "not-a-uuid" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "persisting for an account that is not a uuid",
        )
        .await,
        "Invalid account id."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/apple/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "user_id": player.uid, "kind": "renewal" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an unknown validation kind",
        )
        .await,
        "Validation kind must be \"purchase\" or \"subscription\"."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/nvidia/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "x" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an unknown vendor",
        )
        .await,
        "Unknown IAP vendor: nvidia"
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/samsung/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "receipt": "x", "kind": "subscription" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a subscription for a vendor that has none",
        )
        .await,
        "Samsung subscriptions are not supported."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/iap/google/validate", env.base))
                .bearer_auth(console)
                .json(&json!({ "user_id": player.uid }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a validation with no credential",
        )
        .await,
        "receipt or purchase token is required"
    );
    refusal(
        env.client
            .post(format!("{}/v2/console/iap/google/validate", env.base))
            .bearer_auth(viewer)
            .json(&json!({ "receipt": "x", "persist": false }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "validating as a read-only operator",
    )
    .await;

    // The Google purchase token spelling, persisted for the player.
    let stored = json_body(
        env.client
            .post(format!("{}/v2/console/iap/google/validate", env.base))
            .bearer_auth(console)
            .json(&json!({
                "purchase_token": "e2e-google-token",
                "product_id": "gems.large",
                "user_id": player.uid,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/iap/{vendor}/validate",
    )
    .await;
    assert_eq!(stored["validated_purchases"][0]["product_id"], "gems.large");

    let subscriptions = json_body(
        env.client
            .post(format!("{}/v2/console/iap/google/validate", env.base))
            .bearer_auth(console)
            .json(&json!({
                "purchase_token": "e2e-google-sub-token",
                "product_id": "season.pass",
                "user_id": player.uid,
                "kind": "subscription",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "validating a subscription",
    )
    .await;
    assert_eq!(subscriptions["subscription"]["product_id"], "season.pass");

    let purchases = json_body(
        env.client
            .get(format!("{}/v2/console/purchase?limit=10", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/purchase",
    )
    .await;
    assert_eq!(
        purchases
            .as_array()
            .map(|rows| rows.iter().any(|row| row["product_id"] == "gems.large"))
            .unwrap_or(false),
        true,
        "the validated purchase is in the ledger: {purchases}"
    );
    let subs = json_body(
        env.client
            .get(format!("{}/v2/console/subscription", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/subscription",
    )
    .await;
    let rows = subs.as_array().expect("an array of subscriptions").clone();
    assert_eq!(rows.len(), 1, "{subs}");
    assert_eq!(rows[0]["product_id"], "season.pass");
    assert_eq!(rows[0]["user_id"], player.uid);

    // The product-level delete, then the vendor configuration reset. A blank id
    // (%20) is the only empty one routing can hand the handler at all.
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/subscription/%20", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "deleting every subscription of an unnamed product",
        )
        .await,
        "A product id is required."
    );
    expect(
        env.client
            .delete(format!("{}/v2/console/subscription/season.pass", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/subscription/{product_id}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/subscription/season.pass", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting the same product twice",
        )
        .await,
        "Subscription not found for this product."
    );

    assert_eq!(
        refusal(
            env.client
                .delete(format!("{}/v2/console/iap/google/reset", env.base))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "resetting a vendor that was never configured",
        )
        .await,
        "No stored Google IAP configuration."
    );
    sqlx::query(
        "INSERT INTO setting (name, value) VALUES ('iap.google', '{\"token\":\"old\"}'::jsonb)",
    )
    .execute(&env.pool)
    .await
    .expect("seed a vendor configuration");
    expect(
        env.client
            .delete(format!("{}/v2/console/iap/google/reset", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/iap/{vendor}/reset",
    )
    .await;
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM setting WHERE name = 'iap.google'")
        .fetch_one(&env.pool)
        .await
        .expect("count the vendor setting");
    assert_eq!(left, 0, "the reset clears the stored configuration");
}

/// `PUT`/`GET`/`DELETE /v2/console/setting` — the key/value plane.
async fn setting_surface(env: &Harness, console: &str, viewer: &str) {
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

/// The storage browser: every object of a collection, one object's delete, and a
/// whole-collection clear.
async fn storage_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console storage browser");
    let owner = device_player(&env.client, &env.base, "store").await;
    let collection = format!("e2e_console_{}", letters_suffix());
    put_object(&env, &owner.token, &collection, "alpha", json!({"hp": 100})).await;
    put_object(&env, &owner.token, &collection, "beta", json!({"hp": 50})).await;

    // The player read side, by full path, is exercised by the same fixture.
    let one = json_body(
        env.client
            .get(format!(
                "{}/v2/storage/{}/{}/{}",
                env.base, collection, "alpha", owner.uid
            ))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/storage/{collection}/{key}/{userId}",
    )
    .await;
    assert_eq!(json_string(&one["value"]), json!({"hp": 100}));

    let rows = json_body(
        env.client
            .get(format!("{}/v2/console/storage/{collection}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/storage/{collection}",
    )
    .await;
    let listed = rows.as_array().expect("an array of objects").clone();
    assert_eq!(listed.len(), 2, "{rows}");
    let alpha = listed
        .iter()
        .find(|row| row["key"] == "alpha")
        .expect("the alpha object is listed");
    assert_eq!(alpha["collection"], collection);
    assert_eq!(alpha["user_id"], owner.uid);
    assert_eq!(
        alpha["value"],
        json!({"hp": 100}),
        "the console parses the value back: {listed:?}"
    );

    let collections = json_body(
        env.client
            .get(format!("{}/v2/console/storage/collections", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/storage/collections",
    )
    .await;
    assert!(
        collections
            .as_array()
            .map(|names| names.iter().any(|name| name == &json!(collection)))
            .unwrap_or(false),
        "the new collection is named: {collections}"
    );

    // One object, by the triple that keys it.
    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/storage/object/{collection}/alpha/not-a-uuid",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "deleting an object of an account that is not a uuid",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/storage/object/{collection}/gamma/{}",
                    env.base, owner.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting an object that does not exist",
        )
        .await,
        "Object not found."
    );
    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/storage/object/{collection}/alpha/{}",
                env.base, owner.uid
            ))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "deleting an object as a read-only operator",
    )
    .await;
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/storage/object/{collection}/alpha/{}",
                env.base, owner.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/storage/object/{collection}/{key}/{userId}",
    )
    .await;
    let rows = json_body(
        env.client
            .get(format!("{}/v2/console/storage/{collection}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the collection after the object delete",
    )
    .await;
    assert_eq!(rows.as_array().map(Vec::len), Some(1));

    // Clearing a whole collection is admin-only, and counts what it removed.
    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/storage/collection/{collection}",
                env.base
            ))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "clearing a collection as a read-only operator",
    )
    .await;
    let cleared = json_body(
        env.client
            .delete(format!(
                "{}/v2/console/storage/collection/{collection}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/storage/collection/{collection}",
    )
    .await;
    assert_eq!(cleared["removed"], 1, "{cleared}");
}

/// The roster of a group, and the records of a board — both as the console reads
/// them, after the player plane put the rows there.
async fn group_and_board_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console group roster and board records");
    let leader = device_player(&env.client, &env.base, "gldr").await;
    let member = device_player(&env.client, &env.base, "gmb").await;
    let name = format!("e2e roster {}", letters_suffix());
    let created = json_body(
        env.client
            .post(format!("{}/v2/group", env.base))
            .bearer_auth(&leader.token)
            .json(&json!({ "name": name, "open": true, "max_count": 8 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a group",
    )
    .await;
    let group_id = created["group"]["id"]
        .as_str()
        .expect("group id")
        .to_owned();
    expect(
        env.client
            .post(format!("{}/v2/group/{group_id}/join", env.base))
            .bearer_auth(&member.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the second player joins",
    )
    .await;

    let roster = json_body(
        env.client
            .get(format!("{}/v2/console/group/{group_id}/member", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/group/{id}/member",
    )
    .await;
    let rows = roster.as_array().expect("an array of members").clone();
    assert_eq!(rows.len(), 2, "{roster}");
    let states: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["username"].as_str().unwrap_or_default(),
                row["state"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        states.contains(&(leader.username.as_str(), "superadmin")),
        "the creator leads: {states:?}"
    );
    assert!(
        states.contains(&(member.username.as_str(), "member")),
        "the joiner is a member: {states:?}"
    );

    let board = format!("e2e_console_board_{}", letters_suffix());
    sqlx::query(
        "INSERT INTO leaderboard (id, title, category, duration, sort_order, authoritative) \
         VALUES ($1, $1, 0, 0, 1, false)",
    )
    .bind(&board)
    .execute(&env.pool)
    .await
    .expect("seed a board");
    for (token, score) in [(&leader.token, 300), (&member.token, 900)] {
        json_body(
            env.client
                .post(format!("{}/v2/leaderboard/{board}", env.base))
                .bearer_auth(token)
                .json(&json!({ "score": score }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "write a record",
        )
        .await;
    }

    let records = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/{}/records",
                env.base, board
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/leaderboard/{id}/records",
    )
    .await;
    let rows = records.as_array().expect("an array of records").clone();
    assert_eq!(rows.len(), 2, "{records}");
    assert_eq!(rows[0]["leaderboard_id"], board);
    assert_eq!(
        rows[0]["score"].as_i64(),
        Some(900),
        "the board reads in score order"
    );
    // A board id that resolves to nothing is an empty roster, not an error: the
    // unscoped read is a listing, and there are no records to list.
    let unknown = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/not-a-uuid/records",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reading records of a board that does not exist",
    )
    .await;
    assert_eq!(unknown, json!([]), "an unknown board lists nothing");

    // The same read scoped to a tenant is an ownership check: this board was
    // seeded with no `game_id`, so it belongs to no game and the guard refuses it.
    let owned = create_game(env, console, "Gap Board Game", "custom").await;
    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/v2/console/leaderboard/{}/records?game_id={}",
                    env.base, board, owned.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "reading a board through a game that does not own it",
        )
        .await,
        "Leaderboard not found."
    );
    sqlx::query("UPDATE leaderboard SET game_id = $1 WHERE id = $2")
        .bind(Uuid::parse_str(&owned.id).expect("a game uuid"))
        .bind(&board)
        .execute(&env.pool)
        .await
        .expect("claim the board for the game");
    let scoped = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/{}/records?game_id={}",
                env.base, board, owned.id
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reading the records of the board this game owns",
    )
    .await;
    assert_eq!(scoped.as_array().map(Vec::len), Some(2), "{scoped}");
    // A different game still cannot see it, tenant filter and all.
    let other = create_game(env, console, "Gap Other Game", "custom").await;
    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/v2/console/leaderboard/{}/records?game_id={}",
                    env.base, board, other.id
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "reading a board through the wrong game",
        )
        .await,
        "Leaderboard not found."
    );

    // The console's own record removal: addressed by owner, and it leaves the
    // other player's score alone.
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/leaderboard/{}/owner/not-a-uuid",
                    env.base, board
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "removing the record of an account that is not a uuid",
        )
        .await,
        "Invalid owner ID."
    );
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/leaderboard/{}/owner/{}",
                env.base, board, member.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/leaderboard/{id}/owner/{ownerId}",
    )
    .await;
    let after = json_body(
        env.client
            .get(format!(
                "{}/v2/console/leaderboard/{}/records",
                env.base, board
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the board after the console removed one record",
    )
    .await;
    let left = after.as_array().expect("an array of records").clone();
    assert_eq!(left.len(), 1, "{after}");
    assert_eq!(
        left[0]["owner_id"], leader.uid,
        "the leader's score survives"
    );

    // A read-only operator cannot remove the board itself.
    refusal(
        env.client
            .delete(format!("{}/v2/console/leaderboard/{}", env.base, board))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "deleting a board as a read-only operator",
    )
    .await;
    expect(
        env.client
            .delete(format!("{}/v2/console/leaderboard/{}", env.base, board))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/leaderboard/{id}",
    )
    .await;
}

/// The epoch the schema stores for "never" — an account that has not been banned.
const NEVER: &str = "1970-01-01T00:00:00Z";

/// One account row as the console reads it.
async fn account_row(env: &Harness, token: &str, id: &str) -> Value {
    json_body(
        env.client
            .get(format!("{}/v2/console/account/{id}", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/account/{id}",
    )
    .await
}

/// The console's own staff room: banning a player and lifting it again, the notes
/// pinned to an account, the ACL template documents, and the operators themselves.
async fn operator_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console account ban, notes, acl templates and operators");

    let player = device_player(&env.client, &env.base, "banee").await;

    // An account starts unbanned, which every surface below reads as the epoch.
    assert_eq!(
        account_row(env, console, &player.uid).await["disable_time"],
        NEVER
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/ban",
                    env.base, player.uid
                ))
                .bearer_auth(viewer)
                .json(&json!({ "reason": "cheating" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "banning a player as a read-only operator",
        )
        .await,
        "This console user has read-only access."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/account/not-a-uuid/ban", env.base))
                .bearer_auth(console)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "banning an account that is not a uuid",
        )
        .await,
        "Invalid account id."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/ban",
                    env.base,
                    Uuid::new_v4()
                ))
                .bearer_auth(console)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "banning an account that does not exist",
        )
        .await,
        "Entity not found."
    );
    expect(
        env.client
            .post(format!(
                "{}/v2/console/account/{}/ban",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .json(&json!({ "reason": "exploiting" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/ban",
    )
    .await;
    let banned = account_row(env, console, &player.uid).await;
    assert_ne!(
        banned["disable_time"].as_str().unwrap_or_default(),
        NEVER,
        "the ban is a timestamp, not a flag: {banned}"
    );

    // The notes: refused empty, listed after the write, and removed by id.
    assert_eq!(
        refusal(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/note",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .json(&json!({ "note": "   " }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a note with no text",
        )
        .await,
        "A note cannot be empty."
    );
    expect(
        env.client
            .post(format!(
                "{}/v2/console/account/{}/note",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .json(&json!({ "note": "Reviewed the exploit replay." }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/note",
    )
    .await;
    let notes = json_body(
        env.client
            .get(format!(
                "{}/v2/console/account/{}/note",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/account/{id}/note",
    )
    .await;
    let rows = notes.as_array().expect("an array of notes").clone();
    assert_eq!(rows.len(), 1, "{notes}");
    assert_eq!(rows[0]["note"], "Reviewed the exploit replay.");
    let note_id = rows[0]["id"].as_str().expect("note id").to_owned();

    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/account/{}/note/not-a-uuid",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "removing a note that is not a uuid",
        )
        .await,
        "Invalid note id."
    );
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/account/{}/note/{note_id}",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/account/{id}/note/{noteId}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/account/{}/note/{note_id}",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "removing the same note twice",
        )
        .await,
        "Note not found."
    );

    expect(
        env.client
            .post(format!(
                "{}/v2/console/account/{}/unban",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/account/{id}/unban",
    )
    .await;
    assert_eq!(
        account_row(env, console, &player.uid).await["disable_time"],
        NEVER,
        "the unban writes the epoch back"
    );

    // ACL templates: admin-only, named, and holding a permission document.
    let templates = json_body(
        env.client
            .get(format!("{}/v2/console/acl/template", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/acl/template",
    )
    .await;
    assert!(
        templates.is_array(),
        "the template list is an array: {templates}"
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(viewer)
                .json(&json!({ "name": "nope" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "creating a template as a read-only operator",
        )
        .await,
        "This console action requires the admin role."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "  " }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a template with no name",
        )
        .await,
        "A template needs a name."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "gap-acl-shape", "acl": ["players.read"] }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a template whose acl is not an object",
        )
        .await,
        "A template's acl must be an object."
    );
    let template_name = format!("gap acl {}", letters_suffix());
    let made = json_body(
        env.client
            .post(format!("{}/v2/console/acl/template", env.base))
            .bearer_auth(console)
            .json(&json!({
                "name": template_name,
                "description": "Read one board.",
                "acl": { "players.read": true },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/acl/template",
    )
    .await;
    let template_id = made["id"].as_str().expect("template id").to_owned();
    assert_eq!(made["name"], template_name);
    assert_eq!(made["acl"]["players.read"], true);
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/acl/template", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": template_name }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "a second template with the same name",
        )
        .await,
        "A template with that name already exists."
    );
    let edited = json_body(
        env.client
            .put(format!(
                "{}/v2/console/acl/template/{template_id}",
                env.base
            ))
            .bearer_auth(console)
            .json(&json!({
                "name": format!("{template_name} v2"),
                "description": "Read and write one board.",
                "acl": { "players.read": true, "leaderboards.write": true },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "PUT /v2/console/acl/template/{id}",
    )
    .await;
    assert_eq!(edited["name"], format!("{template_name} v2"));
    assert_eq!(edited["acl"]["leaderboards.write"], true);
    assert_eq!(
        refusal(
            env.client
                .put(format!("{}/v2/console/acl/template/not-a-uuid", env.base))
                .bearer_auth(console)
                .json(&json!({ "name": "whatever" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "editing a template that is not a uuid",
        )
        .await,
        "Invalid template id."
    );
    assert_eq!(
        refusal(
            env.client
                .put(format!(
                    "{}/v2/console/acl/template/{}",
                    env.base,
                    Uuid::new_v4()
                ))
                .bearer_auth(console)
                .json(&json!({ "name": "whatever" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "editing a template that does not exist",
        )
        .await,
        "Template not found."
    );

    // An operator the console created, managed through every one of its routes.
    let ops_name = format!("e2e-cops-{}", letters_suffix());
    let ops_password = "e2e-operator-password";
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/user", env.base))
                .bearer_auth(console)
                .json(&json!({
                    "username": ops_name,
                    "email": format!("{ops_name}@example.com"),
                    "password": ops_password,
                    "role": "superuser",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a role the console does not have",
        )
        .await,
        "The role must be admin, developer, maintainer or readonly."
    );
    let operator = json_body(
        env.client
            .post(format!("{}/v2/console/user", env.base))
            .bearer_auth(console)
            .json(&json!({
                "username": ops_name,
                "email": format!("{ops_name}@example.com"),
                "password": ops_password,
                "role": "admin",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create an admin operator",
    )
    .await;
    let ops_id = operator["id"].as_str().expect("operator id").to_owned();
    assert_eq!(operator["role"], "admin");
    assert_eq!(operator["disabled"], false);

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/user/{ops_id}/password", env.base))
                .bearer_auth(console)
                .json(&json!({ "password": "short" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a password under the console minimum",
        )
        .await,
        "A console password must be at least 12 characters."
    );
    let rotated = "e2e-operator-rotated";
    expect(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/password", env.base))
            .bearer_auth(console)
            .json(&json!({ "password": rotated }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/password",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/authenticate", env.base))
                .json(&json!({ "username": ops_name, "password": ops_password }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "signing in with the replaced password",
        )
        .await,
        "Invalid credentials."
    );
    let ops = console_login_as(env, &ops_name, rotated, json!({})).await;

    // An operator cannot turn a disable on itself, and the bootstrap account has no
    // password row to change at all.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/user/{ops_id}/disable", env.base))
                .bearer_auth(&ops)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an operator disabling itself",
        )
        .await,
        "A console user cannot disable itself."
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/password_change", env.base))
                .bearer_auth(console)
                .json(&json!({ "current_password": rotated, "password": "another-password" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the bootstrap account changing its own password",
        )
        .await,
        "The bootstrap console account has no password to change."
    );
    refusal(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/disable", env.base))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "disabling an operator as a read-only operator",
    )
    .await;
    expect(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/disable", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/disable",
    )
    .await;
    let staff = json_body(
        env.client
            .get(format!("{}/v2/console/user?limit=100", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the operator roster",
    )
    .await;
    let listed = staff
        .as_array()
        .expect("an array of operators")
        .iter()
        .find(|row| row["id"] == json!(ops_id))
        .cloned()
        .unwrap_or_else(|| panic!("the operator is listed: {staff}"));
    assert_eq!(listed["disabled"], true, "the roster says it is off");
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/authenticate", env.base))
                .json(&json!({ "username": ops_name, "password": rotated }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "signing in as a disabled operator",
        )
        .await,
        "Invalid credentials."
    );
    expect(
        env.client
            .post(format!("{}/v2/console/user/{ops_id}/enable", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/user/{id}/enable",
    )
    .await;
    console_login_as(env, &ops_name, rotated, json!({})).await;

    // The signed-in operator who does know their current password: wrong first,
    // then the change itself.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/console/password_change", env.base))
                .bearer_auth(&ops)
                .json(&json!({ "current_password": "not-it-at-all", "password": rotated }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "changing a password with the wrong current one",
        )
        .await,
        "Current password is incorrect."
    );
    let own = format!("{template_name}-pw");
    expect(
        env.client
            .post(format!("{}/v2/console/password_change", env.base))
            .bearer_auth(&ops)
            .json(&json!({ "current_password": rotated, "password": own }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /v2/console/password_change",
    )
    .await;
    console_login_as(env, &ops_name, own.as_str(), json!({})).await;

    // The template is deleted, and cannot be deleted twice.
    expect(
        env.client
            .delete(format!(
                "{}/v2/console/acl/template/{template_id}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/acl/template/{id}",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/acl/template/{template_id}",
                    env.base
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting the same template twice",
        )
        .await,
        "Template not found."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn console_module_iap_settings_and_storage_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    // A second operator, read-only: every write route in this file refuses it.
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

    // The audit trail is the console's own contract: every write above is in it.
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
