use super::*;

#[test]
pub(crate) fn the_games_writes_are_gated_on_the_admin_and_write_roles() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();

    for role in [ConsoleRole::Readonly, ConsoleRole::Developer] {
        let reply = server.post(
            "/v2/console/games",
            Some(&token_for(role)),
            r#"{"name":"Tank Arena"}"#,
        );
        assert_eq!(
            reply.refusal(PERMISSION_DENIED),
            "This console action requires the admin role.",
            "a create from {role:?}"
        );
    }
    let reply = server.delete(
        &format!("/v2/console/games/{id}"),
        Some(&token_for(ConsoleRole::Developer)),
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console action requires the admin role."
    );

    let reply = server.put(
        &format!("/v2/console/games/{id}"),
        Some(&token_for(ConsoleRole::Readonly)),
        r#"{"name":"Renamed"}"#,
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );
    let reply = server.put(
        &format!("/v2/console/games/{id}"),
        Some(&token_for(ConsoleRole::Developer)),
        r#"{"name":"Renamed"}"#,
    );
    assert_eq!(reply.status, 503, "{}", reply.body);

    for path in [
        "/v2/console/games".to_owned(),
        format!("/v2/console/games/{id}"),
    ] {
        let reply = server.get(&path, Some(&token_for(ConsoleRole::Readonly)));
        assert_eq!(reply.status, 503, "{path}: {}", reply.body);
    }
}
#[test]
pub(crate) fn the_unity_settings_route_is_a_read_mounted_in_both_formats() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();

    for role in [
        ConsoleRole::Readonly,
        ConsoleRole::Developer,
        ConsoleRole::Admin,
    ] {
        for path in [
            format!("/v2/console/games/{id}/unity-settings"),
            format!("/v2/console/games/{id}/unity-settings?format=asset"),
            format!("/v2/console/games/{id}/unity-settings?format=json"),
        ] {
            let reply = server.get(&path, Some(&token_for(role)));
            assert_ne!(reply.status, 404, "`{role:?}` cannot reach {path}");
            assert_ne!(
                reply.status, 405,
                "{path} is mounted but does not claim GET: {}",
                reply.body
            );
            assert_eq!(
                reply.refusal(UNAVAILABLE),
                "The database is not configured, set [control.session] database_endpoint.",
                "{role:?} GET {path}"
            );
        }
    }
}
#[test]
pub(crate) fn the_create_body_is_validated_before_the_database_is_reached() {
    let server = V2Server::start(v2_state());
    let auth = admin_token();

    let reply = server.post("/v2/console/games", Some(&auth), r#"{"name":""}"#);
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "A game needs a name.");

    let reply = server.post("/v2/console/games", Some(&auth), "{}");
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game needs a name.",
        "a create that names no field is refused for the field that matters"
    );

    let reply = server.post(
        "/v2/console/games",
        Some(&auth),
        r#"{"name":"Tank Arena","engine_type":"phaser"}"#,
    );
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(
        message.contains("Unknown engine type 'phaser'"),
        "{message}"
    );
    for engine in ENGINE_TYPES {
        assert!(
            message.contains(engine),
            "an engine refusal names the whole whitelist, {engine} missing from {message}"
        );
    }

    let reply = server.post(
        "/v2/console/games",
        Some(&auth),
        r#"{"name":"Tank Arena","config":[1,2,3]}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game config must be a JSON object."
    );

    for body in [
        r#"{"name":"Tank Arena","engine_type":"unity"}"#,
        r#"{"name":"Tank Arena","engine_type":"custom","tick_rate":60,"max_clients":8,
           "persistent_rooms":true,"auto_close_seconds":12}"#,
        r#"{"name":"Tank Arena","engine_type":"web","config":{"tick_rate":30}}"#,
        r#"{"name":"Tank Arena","engine_type":"unity","backend_mode":"wasm",
           "api_key":"cyg_sec_preselected"}"#,
    ] {
        let reply = server.post("/v2/console/games", Some(&auth), &collapse(body));
        assert_eq!(
            reply.refusal(UNAVAILABLE),
            "The database is not configured, set [control.session] database_endpoint.",
            "a well formed create {body} is stopped by the missing database, not by \
             its body"
        );
    }
}
