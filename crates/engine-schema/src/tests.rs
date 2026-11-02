use super::*;

#[test]
fn hash_matches_the_js_and_assemblyscript_runtimes() {
    let parsed = ts::parse(
        r#"
        export const Player = schema({
            x: t.number(),
            y: t.number(),
            flags: t.uint32(),
        }, "Player");
        "#,
    )
    .unwrap();

    assert_eq!(wire_hash(&parsed.schemas[0]), 1_154_959_100);
}

#[test]
fn descriptor_carries_schema_fields_hash_and_root_collection() {
    let parsed = ts::parse(
        r#"
        export const Player = schema({
            x: t.number(),
            y: t.number(),
            flags: t.uint32(),
        }, "Player");
        "#,
    )
    .unwrap();
    let schemas = parsed.schemas;
    let root = RootState {
        name: "State".into(),
        version: 1,
        scalars: Vec::new(),
        collections: vec![RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    };
    let bytes = emit_descriptor(&schemas, Some(&root));
    assert_eq!(&bytes[..4], b"OXSD");
    assert_eq!(bytes[4], DESCRIPTOR_VERSION);
    assert!(
        bytes.windows(8).any(|window| window == b"\x06\x00Player"),
        "schema name is length-prefixed"
    );
    assert!(
        bytes.windows(9).any(|window| window == b"\x07\x00players"),
        "root collection is recorded"
    );
    let descriptor = generate_with_root(&schemas, Some(&root), "schema.ts").descriptor;
    assert_eq!(descriptor, bytes);
}

#[test]
fn ts_schema_generates_typed_class_defaults_and_descriptor() {
    let parsed = ts::parse(
        r#"
        export const Player = schema({
            x: t.number(),
            hp: t.uint8().default(100),
            flags: t.uint32(),
        }, "Player");
        export const State = schema({ players: t.map(Player) }, "State");
        "#,
    )
    .unwrap();
    let generated = generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
    assert!(generated.server_ts.contains(".f64(\"x\")"));
    assert!(generated.server_ts.contains(".u8(\"hp\")"));
    assert!(generated.server_ts.contains("this.setU8(_Player_hp, 100);"));
    assert_eq!(&generated.descriptor[..4], b"OXSD");
}
