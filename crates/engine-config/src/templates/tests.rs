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
    assert!(ROOM_TEMPLATE.contains("onInputPacket"));
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
