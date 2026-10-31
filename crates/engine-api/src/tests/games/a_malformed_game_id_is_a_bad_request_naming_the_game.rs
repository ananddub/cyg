use super::*;

#[test]
pub(crate) fn a_malformed_game_id_is_a_bad_request_naming_the_game() {
    let server = V2Server::start(v2_state());
    let auth = admin_token();
    for (method, path) in [
        ("GET", "/v2/console/games/not-a-uuid".to_owned()),
        ("PUT", "/v2/console/games/not-a-uuid".to_owned()),
        ("DELETE", "/v2/console/games/not-a-uuid".to_owned()),
        (
            "GET",
            "/v2/console/games/not-a-uuid/unity-settings".to_owned(),
        ),
        (
            "GET",
            "/v2/console/games/not-a-uuid/unity-settings?format=json".to_owned(),
        ),
    ] {
        let reply = server.send(&method, &path, Some(&auth), "{}");
        assert_eq!(
            reply.refusal(INVALID_ARGUMENT),
            "Invalid game id.",
            "{method} {path}"
        );
    }
}
#[test]
pub(crate) fn an_empty_patch_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let reply = server.put(
        &format!("/v2/console/games/{}", Uuid::new_v4()),
        Some(&admin_token()),
        "{}",
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game update has to change at least one field."
    );
}
#[test]
pub(crate) fn an_unknown_game_status_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let reply = server.put(
        &format!("/v2/console/games/{}", Uuid::new_v4()),
        Some(&admin_token()),
        r#"{"status":"deleted"}"#,
    );
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(
        message.contains("Unknown game status 'deleted'"),
        "{message}"
    );
    for status in GAME_STATUSES {
        assert!(message.contains(status), "{status} missing from {message}");
    }
}
#[test]
pub(crate) fn an_overlong_game_name_is_refused() {
    let server = V2Server::start(v2_state());
    let name = "n".repeat(129);
    let reply = server.post(
        "/v2/console/games",
        Some(&admin_token()),
        &format!(r#"{{"name":"{name}"}}"#),
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game name is at most 128 characters."
    );
}
pub(crate) const UNITY_SDK_SOURCE: &str =
    include_str!("../../../../../sdk/client/unity/Runtime/Core/CYGServerSettings.cs");
pub(crate) const GAMES_CONTROLLER_SOURCE: &str = concat!(
    include_str!("../../controllers/console/games/unity_asset_yaml.rs"),
    include_str!("../../controllers/console/games/query_params.rs"),
);
pub(crate) fn sdk_serialized_fields() -> Vec<String> {
    UNITY_SDK_SOURCE
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("[SerializeField]")?;
            let name = rest.split_whitespace().nth(1)?.trim_end_matches(';');
            (!name.is_empty()
                && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .then(|| name.to_owned())
        })
        .collect()
}
pub(crate) fn asset_template_fields() -> Vec<String> {
    asset_template_body()
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line[2..].split_once(':'))
        .map(|(key, _value)| key.trim().to_owned())
        .filter(|key| !key.is_empty() && !key.contains('_') && key.contains(char::is_alphanumeric))
        .collect()
}
pub(crate) fn asset_template_body() -> String {
    let start = GAMES_CONTROLLER_SOURCE
        .find("fn unity_asset_yaml")
        .expect("the controller owns the asset template");
    GAMES_CONTROLLER_SOURCE[start..]
        .lines()
        .take_while(|line| !line.starts_with('}'))
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
pub(crate) fn the_asset_template_serializes_exactly_the_fields_the_sdk_class_declares() {
    let declared = sdk_serialized_fields();
    assert_eq!(
        declared,
        [
            "serverHttpUrl".to_owned(),
            "serverWsPath".to_owned(),
            "roomName".to_owned(),
            "maxPlayers".to_owned(),
            "protocolVersion".to_owned(),
            "matchmakeMethod".to_owned(),
        ],
        "the SDK's serialized fields are the six the asset has to carry"
    );

    let emitted = asset_template_fields();
    for name in &declared {
        assert!(
            emitted.iter().any(|field| field == name),
            "the asset template must write `{name}`, which the SDK class declares; \
             it writes {emitted:?}"
        );
    }
    for field in &emitted {
        assert!(
            declared.iter().any(|name| name == field),
            "the asset template writes `{field}`, which `CYGServerSettings` does not \
             declare — Unity drops it: {emitted:?}"
        );
    }
    assert_eq!(
        emitted.len(),
        declared.len(),
        "and the two lists are the same length: no field is spelled twice"
    );
}
