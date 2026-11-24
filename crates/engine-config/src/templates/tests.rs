use super::*;

#[test]
fn easy_mode_defaults_to_30hz_and_20hz_remains_available() {
    let cfg = crate::parse_config(CONFIG_TEMPLATE);
    assert_eq!(
        cfg.tick_rate, 30,
        "the generated project is easy-mode 30 Hz"
    );
    assert!(
        cfg.permissions.messaging,
        "typed messages are on by default"
    );

    let twenty = crate::parse_config("tick_rate = 20\n");
    assert_eq!(twenty.tick_rate, 20, "20 Hz is still a supported choice");
}

#[test]
fn room_template_is_a_class_with_all_three_channels() {
    assert!(ROOM_TEMPLATE.contains("export class MyGame extends Room"));

    assert!(
        ROOM_TEMPLATE.contains("onInputFrame(client: Client, f: Input)"),
        "the input frame arrives pre-decoded"
    );
    assert!(ROOM_TEMPLATE.contains("commitState("));
    assert!(ROOM_TEMPLATE.contains("onJoin"));
    assert!(ROOM_TEMPLATE.contains("from \"../gen/cyg.schema\""));
    assert!(ROOM_TEMPLATE.contains("new SchemaState(PlayerSchema)"));
    assert!(!ROOM_TEMPLATE.contains("new SchemaDef("));
    assert!(ROOM_TEMPLATE.contains("return this.commitState();"));
}

#[test]
fn ts_schema_template_uses_colyseus_v5_syntax() {
    assert!(SCHEMA_TS_TEMPLATE.contains("from \"cyg\""));
    assert!(SCHEMA_TS_TEMPLATE.contains("const Player = schema({"));
    assert!(SCHEMA_TS_TEMPLATE.contains("players: t.map(Player)"));
}

#[test]
fn ts_schema_template_declares_the_input_frame() {
    assert!(
        SCHEMA_TS_TEMPLATE.contains("const Input = schema({"),
        "the scaffold declares the typed input lane"
    );
    assert!(
        ROOM_TEMPLATE.contains("onInputFrame(client: Client"),
        "the room receives decoded input frames via the client handle"
    );
    assert!(
        ROOM_TEMPLATE.contains("onCreate(max: i32, options: string)"),
        "onCreate receives the merged create options"
    );
    assert!(
        ROOM_TEMPLATE.contains("onLeave(client: Client, code: i32)"),
        "onLeave carries the CloseCode"
    );
    assert!(
        ROOM_TEMPLATE.contains("onJoin(client: Client"),
        "the room uses the Colyseus-shaped client handle"
    );
}

#[test]
fn ts_schema_template_declares_typed_join_options() {
    assert!(
        SCHEMA_TS_TEMPLATE.contains("const JoinOptions = schema({"),
        "the scaffold declares typed join options"
    );
    assert!(
        ROOM_TEMPLATE.contains("onJoin(client: Client, options: JoinOptions, auth: string)"),
        "the room's onJoin is typed"
    );
    assert!(
        !ROOM_TEMPLATE.contains("jsonString("),
        "the scaffold needs no JSON helpers"
    );
}

#[test]
fn views_permission_is_deny_by_default_and_configurable() {
    assert!(!crate::GameConfig::default().permissions.views);
    let cfg = crate::parse_config("allow_views = true\n");
    assert!(cfg.permissions.views);
    assert!(!crate::GameConfig::default().permissions.rewind);
    assert!(
        crate::parse_config("allow_rewind = true\n")
            .permissions
            .rewind
    );
}
