use crate::ts::parse;
use crate::{FIELD_OPTIONAL, FIELD_STATIC, WireKind};

#[test]
fn parses_colyseus_v5_schema_and_root_collection() {
    let source = r#"
        import { schema, t } from "./cyg-server";

        export const Player = schema({
            x: t.number(),
            y: t.number(),
            flags: t.uint32(),
        }, "Player");

        export const State = schema({
            players: t.map(Player),
        }, "State");
    "#;
    let parsed = parse(source).expect("schema.ts parses");
    assert_eq!(parsed.schemas.len(), 1);
    let player = &parsed.schemas[0];
    assert_eq!(player.name, "Player");
    assert_eq!(player.version, 1);
    assert_eq!(player.fields.len(), 3);
    assert_eq!(player.fields[0].kind, WireKind::F64);
    assert_eq!(player.fields[2].kind, WireKind::U32);
    let root = parsed.root.expect("root world");
    assert_eq!(root.name, "State");
    assert_eq!(root.collections.len(), 1);
    assert_eq!(root.collections[0].name, "players");
    assert_eq!(root.collections[0].schema, "Player");
}

#[test]
fn infers_schema_name_from_const_binding() {
    let parsed = parse("const Tiny = schema({ hp: t.uint8() });").unwrap();
    assert_eq!(parsed.schemas[0].name, "Tiny");
}

#[test]
fn accepts_aliases_refs_and_defaults() {
    let source = r#"
        const Player = schema({
            hp: t.uint8().default(100),
            shield: t.int16().default(-5),
            speed: t.float32(),
            target: t.ref(Player),
            alive: t.boolean().default(true),
        }, "Player");
    "#;
    let parsed = parse(source).unwrap();
    let fields = &parsed.schemas[0].fields;
    assert_eq!(fields[0].kind, WireKind::U8);
    assert_eq!(fields[0].default.as_deref(), Some("100"));
    assert_eq!(fields[1].kind, WireKind::I16);
    assert_eq!(fields[1].default.as_deref(), Some("-5"));
    assert_eq!(fields[2].kind, WireKind::F32);
    assert_eq!(fields[3].kind, WireKind::U32);
    assert_eq!(fields[4].default.as_deref(), Some("true"));
}

#[test]
fn parses_strings_flags_arrays_and_function_defaults() {
    let source = r#"
        const Player = schema({
            name: t.string().default("rookie"),
            hp: t.uint8().default(() => 100),
            tag: t.string().optional(),
            score: t.uint32().static(),
            seen: t.array(t.float32()),
            target: t.ref(Player),
        }, "Player");
    "#;
    let parsed = parse(source).expect("phase 3 fields parse");
    let fields = &parsed.schemas[0].fields;
    assert_eq!(fields[0].kind, WireKind::String);
    assert_eq!(fields[0].default.as_deref(), Some("rookie"));
    assert_eq!(fields[1].default.as_deref(), Some("100"));
    assert_eq!(fields[2].kind, WireKind::String);
    assert_eq!(fields[2].flags & FIELD_OPTIONAL, FIELD_OPTIONAL);
    assert_eq!(fields[3].flags & FIELD_STATIC, FIELD_STATIC);
    assert_eq!(fields[4].kind, WireKind::Array);
    assert_eq!(fields[4].element, Some(WireKind::F32));
    assert_eq!(fields[5].kind, WireKind::U32);
    assert_eq!(fields[5].ref_target.as_deref(), Some("Player"));
}

#[test]
fn ignores_comments_and_non_schema_code() {
    let source = r#"
        // const Old = schema({ nope: t.string() }, "Old");
        /* schema({ nope: t.string() }, "Old"); */
        const label = "schema({ nope: t.string() }, \"Old\")";
        export const Player = schema({ x: t.number() }, "Player");
    "#;
    let parsed = parse(source).unwrap();
    assert_eq!(parsed.schemas.len(), 1);
    assert_eq!(parsed.schemas[0].name, "Player");
}
