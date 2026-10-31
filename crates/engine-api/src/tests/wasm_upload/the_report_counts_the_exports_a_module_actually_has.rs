use super::*;

#[test]
pub(crate) fn the_report_counts_the_exports_a_module_actually_has() {
    let declared = abi_exports();
    let (full, _) = complete_wasm();
    let partial = module_with_exports(&declared[..declared.len() - 1]);

    let full_report = WasmService::validate_binary(&full);
    let partial_report = WasmService::validate_binary(&partial);
    assert_eq!(full_report.export_count as usize, declared.len());
    assert_eq!(partial_report.export_count as usize, declared.len() - 1);
    assert_eq!(
        partial_report.exports_missing.len(),
        1,
        "one export short of the ABI is one missing handler"
    );
    assert_eq!(
        partial_report.exports_missing[0],
        declared[declared.len() - 1],
        "and it is the one that was left off"
    );
}
#[test]
pub(crate) fn an_activate_request_carries_only_a_version() {
    let request: ActivateRequest = serde_json::from_value(serde_json::json!({"version": 2}))
        .expect("the documented body deserializes");
    assert_eq!(request.version, 2);

    let greedy: ActivateRequest = serde_json::from_value(serde_json::json!({
        "version": 7,
        "filename": "room.wasm",
        "binary_bytes": "AAAA",
    }))
    .expect("extra fields are dropped rather than refused");
    let printed = format!("{greedy:?}");
    assert_eq!(greedy.version, 7);
    assert!(
        !printed.contains("room.wasm") && !printed.contains("AAAA"),
        "an activation cannot smuggle a payload: {printed}"
    );

    let empty: ActivateRequest =
        serde_json::from_value(serde_json::json!({})).expect("the body has no required field");
    assert_eq!(empty.version, 0);
}
#[test]
pub(crate) fn every_wasm_route_demands_a_console_token() {
    let server = V2Server::start(v2_state());
    for (method, path) in wasm_paths("00000000-0000-0000-0000-000000000000") {
        let reply = server.send(&method, &path, None, "{}");
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Console token required",
            "{method} {path}"
        );
    }
}
pub(crate) fn wasm_paths(game: &str) -> Vec<(&'static str, String)> {
    let base = format!("/v2/console/games/{game}/wasm");
    vec![
        ("GET", base.clone()),
        ("POST", base.clone()),
        ("POST", format!("{base}/upload")),
        ("POST", format!("{base}/activate")),
        ("POST", format!("{base}/rollback")),
        ("DELETE", format!("{base}/1")),
    ]
}
#[test]
pub(crate) fn a_malformed_module_game_id_is_a_bad_request_naming_the_game() {
    let server = V2Server::start(v2_state());
    let auth = format!(
        "Bearer {}",
        console::mint("", "wasm-probe", "wasm@console.local", ConsoleRole::Admin)
    );
    for (method, path) in wasm_paths("not-a-uuid") {
        let body = if method == "DELETE" || method == "GET" {
            String::new()
        } else {
            r#"{"version":1}"#.to_owned()
        };
        let reply = server.send(&method, &path, Some(&auth), &body);
        assert_eq!(
            reply.refusal(INVALID_ARGUMENT),
            "Invalid game id.",
            "{method} {path}"
        );
    }
}
#[test]
pub(crate) fn the_wasm_routes_are_mounted_under_their_game() {
    let server = V2Server::start(v2_state());
    let auth = format!(
        "Bearer {}",
        console::mint("", "wasm-probe", "wasm@console.local", ConsoleRole::Admin)
    );
    let game = uuid::Uuid::new_v4().to_string();
    let paths = wasm_paths(&game);

    for (method, path) in paths {
        let uploads = path.ends_with("/upload") || path.ends_with("/wasm");
        let body = match (method.as_ref(), uploads) {
            ("POST", true) => "{}".to_owned(),
            ("POST", false) => r#"{"version":1}"#.to_owned(),
            _ => String::new(),
        };
        let reply = server.send(&method, &path, Some(&auth), &body);
        assert_ne!(reply.status, 404, "{method} {path} is not mounted at all");
        assert_ne!(
            reply.status, 405,
            "{method} {path} is mounted but does not claim the method: {}",
            reply.body
        );
        if uploads && method == "POST" {
            let message = reply.refusal(INVALID_ARGUMENT);
            assert!(
                message.to_lowercase().contains("multipart"),
                "{method} {path} is mounted and refuses an unformed body by naming \
                 the framing: {message}"
            );
        } else {
            assert_eq!(
                reply.refusal(crate::error::UNAVAILABLE),
                "The database is not configured, set [control.session] database_endpoint.",
                "{method} {path}"
            );
        }
    }
}
#[test]
pub(crate) fn the_wasm_listing_reaches_the_database_it_needs() {
    let server = V2Server::start(v2_state());
    let auth = format!(
        "Bearer {}",
        console::mint("", "wasm-probe", "wasm@console.local", ConsoleRole::Admin)
    );
    let game = uuid::Uuid::new_v4();
    let reply = server.get(&format!("/v2/console/games/{game}/wasm"), Some(&auth));
    assert_eq!(
        reply.refusal(crate::error::UNAVAILABLE),
        "The database is not configured, set [control.session] database_endpoint."
    );
}
