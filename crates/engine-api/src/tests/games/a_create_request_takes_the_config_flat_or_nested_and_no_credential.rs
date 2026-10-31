use super::*;

#[test]
pub(crate) fn a_create_request_takes_the_config_flat_or_nested_and_no_credential() {
    let bare: CreateGameRequest = serde_json::from_value(json!({"name": "Tank Arena"}))
        .expect("a name alone is a complete create");
    assert_eq!(bare.name, "Tank Arena");
    assert_eq!(bare.engine_type, None);
    assert_eq!(bare.config, None);

    let nested: CreateGameRequest = serde_json::from_value(json!({
        "name": "Tank Arena",
        "engine_type": "godot",
        "config": {"tick_rate": 64},
    }))
    .expect("the nested spelling deserializes");
    assert_eq!(nested.engine_type.as_deref(), Some("godot"));
    let config = nested
        .config
        .expect("a named config is kept as the caller wrote it");
    assert_eq!(config["tick_rate"], json!(64));

    let flat: CreateGameRequest = serde_json::from_value(json!({
        "name": "Tank Arena",
        "engine_type": "web",
        "tick_rate": 60,
        "max_clients": 8,
        "persistent_rooms": true,
        "auto_close_seconds": 12,
    }))
    .expect("the flat spelling deserializes");
    let printed = format!("{flat:?}");
    assert_eq!(flat.engine_type.as_deref(), Some("web"));
    for (key, value) in [
        ("tick_rate", "60"),
        ("max_clients", "8"),
        ("persistent_rooms", "true"),
        ("auto_close_seconds", "12"),
    ] {
        assert!(
            printed.contains(key) && printed.contains(value),
            "a flat `{key}` has to be kept by the create, not dropped as an unknown \
             field: {printed}"
        );
    }

    let hostile: CreateGameRequest = serde_json::from_value(json!({
        "name": "Tank Arena",
        "game_key": "gm_mine_000001",
        "api_key": "cyg_sec_preselected",
        "client_key": "cyg_pub_preselected",
        "backend_mode": "wasm",
    }))
    .expect("unknown fields are dropped, not an error");
    let printed = format!("{hostile:?}");
    for smuggled in ["gm_mine", "cyg_sec", "cyg_pub", "wasm"] {
        assert!(
            !printed.contains(smuggled),
            "a create must not be able to carry `{smuggled}`: {printed}"
        );
    }
    assert_eq!(hostile.name, "Tank Arena");
}
#[test]
pub(crate) fn an_update_request_is_a_patch_over_optional_fields() {
    let empty: UpdateGameRequest =
        serde_json::from_value(json!({})).expect("every patch field is optional");
    assert_eq!(empty.name, None);
    assert_eq!(empty.engine_type, None);
    assert_eq!(empty.status, None);
    assert_eq!(empty.config, None);

    let renamed: UpdateGameRequest =
        serde_json::from_value(json!({"name": "Second Name"})).expect("a name-only patch");
    assert_eq!(renamed.name.as_deref(), Some("Second Name"));
    assert_eq!(
        renamed.status, None,
        "an absent status is not a status change"
    );

    let hostile: UpdateGameRequest = serde_json::from_value(json!({
        "name": "Second Name",
        "backend_mode": "wasm",
        "api_key": "cyg_sec_rotated_by_me",
        "client_key": "cyg_pub_rotated_by_me",
        "game_key": "gm_renamed",
    }))
    .expect("a patch with fields the shape has no room for is still a patch");
    let printed = format!("{hostile:?}");
    for smuggled in ["wasm", "cyg_sec", "cyg_pub", "gm_renamed"] {
        assert!(
            !printed.contains(smuggled),
            "a patch cannot carry `{smuggled}`; the mode is derived and the \
             credentials are minted: {printed}"
        );
    }
    assert_eq!(hostile.name.as_deref(), Some("Second Name"));
}
pub(crate) fn games_routes() -> Vec<(&'static str, String)> {
    let id = "00000000-0000-0000-0000-000000000000";
    vec![
        ("GET", "/v2/console/games".to_owned()),
        ("POST", "/v2/console/games".to_owned()),
        ("GET", format!("/v2/console/games/{id}")),
        ("PUT", format!("/v2/console/games/{id}")),
        ("DELETE", format!("/v2/console/games/{id}")),
    ]
}
pub(crate) fn unity_settings_routes() -> Vec<(&'static str, String)> {
    let id = "00000000-0000-0000-0000-000000000000";
    vec![
        ("GET", format!("/v2/console/games/{id}/unity-settings")),
        (
            "GET",
            format!("/v2/console/games/{id}/unity-settings?format=asset"),
        ),
        (
            "GET",
            format!("/v2/console/games/{id}/unity-settings?format=json"),
        ),
    ]
}
#[test]
pub(crate) fn every_games_route_demands_a_console_token() {
    let server = V2Server::start(v2_state());
    for (method, path) in games_routes().into_iter().chain(unity_settings_routes()) {
        let reply = server.send(&method, &path, None, "{}");
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Console token required",
            "{method} {path}"
        );
    }
}
