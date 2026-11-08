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

const INPUT_PROJECT: &str = r#"
export const Player = schema({ x: t.number(), y: t.number() }, "Player");
export const State = schema({ players: t.map(Player) }, "State");
export const Input = schema({
    moveX: t.f32().default(0),
    moveZ: t.f32().default(0),
    jump: t.boolean().default(false),
    skill: t.uint8().default(1),
}, "Input");
"#;

#[test]
fn input_def_emits_typed_decoder() {
    let parsed = ts::parse(INPUT_PROJECT).unwrap();
    let generated = generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");

    // One import statement from the SDK, merged (duplicate imports break asc).
    assert!(
        generated
            .server_ts
            .contains("import { SchemaDef, SchemaEntity, InputReader } from \"./cyg-server\";")
    );
    // The exported decoder returns the generated class, reading each field in
    // schema order.
    assert!(
        generated
            .server_ts
            .contains("export function decodeInput(r: InputReader): Input {")
    );
    assert!(generated.server_ts.contains("f.moveX = r.readF32();"));
    assert!(generated.server_ts.contains("f.jump = r.readBool();"));
    assert!(generated.server_ts.contains("f.skill = r.readU8();"));
    // The Input def stays in the descriptor — the client packs from it by name.
    let descriptor =
        generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts").descriptor;
    assert!(
        descriptor
            .windows(7)
            .any(|window| window == b"\x05\x00Input")
    );
}

#[test]
fn project_without_input_def_emits_no_decoder() {
    let parsed = ts::parse(
        r#"
        export const Player = schema({ x: t.number() }, "Player");
        export const State = schema({ players: t.map(Player) }, "State");
        "#,
    )
    .unwrap();
    let generated = generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
    assert!(!generated.server_ts.contains("decodeInput"));
    assert!(!generated.server_ts.contains("InputReader"));
}

#[test]
fn input_frame_rejects_variable_size_fields() {
    let error = ts::parse(
        r#"
        export const Player = schema({ x: t.number() }, "Player");
        export const State = schema({ players: t.map(Player) }, "State");
        export const Input = schema({ name: t.string() }, "Input");
        "#,
    )
    .unwrap_err();
    assert!(
        error
            .iter()
            .any(|diagnostic| diagnostic.message.contains("input frame field `name`")),
        "string field in the input frame is rejected: {error:?}"
    );
}

#[test]
fn input_frame_name_cannot_double_as_state_entity() {
    let error = ts::parse(
        r#"
        export const Input = schema({ moveX: t.f32() }, "Input");
        export const State = schema({ players: t.map(Input) }, "State");
        "#,
    )
    .unwrap_err();
    assert!(
        error.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot also be a state collection")),
        "Input referenced as a collection is rejected: {error:?}"
    );
}

const JOIN_PROJECT: &str = r#"
export const Player = schema({ x: t.number(), y: t.number() }, "Player");
export const State = schema({ players: t.map(Player) }, "State");
export const JoinOptions = schema({
    name: t.string(),
    team: t.uint8().default(0),
    level: t.uint16().default(1),
}, "JoinOptions");
"#;

fn join_options_fields() -> Vec<SchemaField> {
    ts::parse(JOIN_PROJECT)
        .unwrap()
        .schemas
        .into_iter()
        .find(|schema| schema.name == "JoinOptions")
        .unwrap()
        .fields
}

#[test]
fn join_options_emit_typed_decoder_and_descriptor() {
    let parsed = ts::parse(JOIN_PROJECT).unwrap();
    let generated = generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
    assert!(
        generated
            .server_ts
            .contains("export function decodeJoinOptions(r: InputReader): JoinOptions {")
    );
    assert!(generated.server_ts.contains("o.name = r.readString();"));
    assert!(generated.server_ts.contains("o.team = r.readU8();"));
    // The def rides the descriptor so the client/host can see the contract.
    let descriptor = generated.descriptor;
    assert!(
        descriptor
            .windows(13)
            .any(|window| window == b"\x0b\x00JoinOptions")
    );
}

#[test]
fn join_options_name_cannot_double_as_state_entity() {
    let error = ts::parse(
        r#"
        export const JoinOptions = schema({ name: t.string() }, "JoinOptions");
        export const State = schema({ players: t.map(JoinOptions) }, "State");
        "#,
    )
    .unwrap_err();
    assert!(
        error.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot also be a state collection")),
        "JoinOptions referenced as a collection is rejected: {error:?}"
    );
}

#[test]
fn join_options_encoder_mirrors_the_generated_decoder() {
    let fields = join_options_fields();

    // Full payload: string + u8 + u16, in field order.
    let bytes = encode_join_options(&fields, r#"{"name":"miya","team":2,"level":7}"#);
    assert_eq!(bytes[0], 4, "string length prefix (u16 LE low byte)");
    assert_eq!(bytes[1], 0, "string length prefix high byte");
    assert_eq!(&bytes[2..6], b"miya");
    assert_eq!(bytes[6], 2, "team u8");
    assert_eq!(u16::from_le_bytes([bytes[7], bytes[8]]), 7, "level u16");
    assert_eq!(bytes.len(), 9);

    // Missing keys become defaults (0); wrong types too.
    let bytes = encode_join_options(&fields, r#"{"team":true}"#);
    assert_eq!(&bytes[..2], &[0, 0], "missing name encodes empty string");
    assert_eq!(bytes[2], 0, "true is not a u8 — default 0");
    assert_eq!(u16::from_le_bytes([bytes[3], bytes[4]]), 1, "level default");

    // Garbage JSON encodes as all-defaults instead of failing the join.
    let bytes = encode_join_options(&fields, "not json at all");
    assert_eq!(bytes.len(), 2 + 1 + 2);
    assert_eq!(&bytes[..2], &[0, 0]);
}
