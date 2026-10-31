use super::*;

#[test]
pub(crate) fn the_asset_template_is_a_scriptable_object_document_of_the_sdk_class() {
    let template = asset_template_body();

    for marker in [
        "%YAML 1.1",
        "%TAG !u! tag:unity3d.com,2011:",
        "--- !u!114 &11400000",
        "MonoBehaviour:",
        "m_Script:",
        "m_Name: CYGServerSettings",
    ] {
        assert!(
            template.contains(marker),
            "a Unity asset opens with `{marker}`; the template does not emit it"
        );
    }

    let line = template
        .lines()
        .find(|line| line.trim_start().starts_with("protocolVersion:"))
        .expect("the template writes protocolVersion");
    let value = line.split_once(':').expect("a key and a value").1.trim();
    assert!(
        value.starts_with('{') && value.ends_with('}'),
        "`{value}` is substituted from the generator's own constant"
    );
    let constant = value.trim_matches('{').trim_matches('}').trim();
    let declaration = format!("const {constant}: u16");
    assert!(
        GAMES_CONTROLLER_SOURCE.contains(&declaration),
        "protocol version must be a numeric constant, declared as `{declaration}`"
    );
    let numeric: u32 = GAMES_CONTROLLER_SOURCE
        .lines()
        .find(|line| line.contains(&declaration))
        .and_then(|line| {
            line.split_once('=')?
                .1
                .trim()
                .trim_end_matches(';')
                .parse()
                .ok()
        })
        .expect("and its value is a number, not an expression");
    assert!(
        numeric <= u32::from(u16::MAX),
        "and it fits the `ushort` the SDK field is: {numeric}"
    );

    let names_the_asset = GAMES_CONTROLLER_SOURCE.contains("attachment")
        && GAMES_CONTROLLER_SOURCE.contains("filename=")
        && GAMES_CONTROLLER_SOURCE.contains("CYGServerSettings.asset");
    assert!(
        names_the_asset,
        "the asset must arrive as `CYGServerSettings.asset`, or a build drops it into \
         the project under a name the SDK does not expect"
    );
}
#[test]
pub(crate) fn the_settings_defaults_are_the_ones_the_sdk_declares() {
    assert!(
        UNITY_SDK_SOURCE.contains("protocolVersion = 1"),
        "the SDK class defaults `protocolVersion` to 1; if that changed, so did the \
         asset this file checks"
    );

    assert!(
        UNITY_SDK_SOURCE.contains("roomName = \"\""),
        "the SDK class leaves `roomName` empty; the asset, not the class, owns the \
         room default"
    );

    let client = include_str!("../../../../../sdk/client/unity/Runtime/CygClient.cs");
    assert!(
        client.contains("\"joinOrCreate\""),
        "the SDK's join path calls matchmake with `joinOrCreate` — the connection \
         document's default has to be the client's own word"
    );

    let service = include_str!("../../services/games/service/q.rs");
    assert!(
        service.contains("DEFAULT_ROOM_NAME: &str = \"cyg_room\"")
            && service.contains("SERVER_WS_PATH: &str = \"/ws\""),
        "the service restates the room default and the plane's `/ws` mount"
    );
    assert!(
        GAMES_CONTROLLER_SOURCE.contains("MATCHMAKE_METHOD: &str = \"joinOrCreate\""),
        "the controller emits the SDK's own join method, not a word it invented"
    );
}
