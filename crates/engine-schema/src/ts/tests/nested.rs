use crate::ts::parse;
use crate::{DESCRIPTOR_VERSION, emit_descriptor, parse_descriptor};

const NESTED: &str = r#"
    const Stats = schema({
        kills: t.uint32(),
        deaths: t.uint32().default(0),
    }, "Stats");

    const Player = schema({
        name: t.string(),
        stats: Stats,
    }, "Player");
"#;

#[test]
fn a_nested_schema_is_flattened_with_its_path_kept() {
    let parsed = parse(NESTED).expect("nested schema parses");
    let player = parsed
        .schemas
        .iter()
        .find(|schema| schema.name == "Player")
        .expect("Player exists");
    let names: Vec<&str> = player.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["name", "stats__kills", "stats__deaths"]);
    assert_eq!(player.fields[1].nested_path.as_deref(), Some("stats"));
    assert_eq!(player.fields[2].nested_path.as_deref(), Some("stats"));
    assert_eq!(player.fields[2].default.as_deref(), Some("0"));
}

#[test]
fn nested_codegen_exposes_the_parent_accessor() {
    let parsed = parse(NESTED).unwrap();
    let generated = crate::emit_server(&parsed.schemas, parsed.root.as_ref(), "nested.ts");
    assert!(
        generated.contains("get stats(): Player_Stats"),
        "{generated}"
    );
    assert!(
        generated.contains("export class Player_Stats"),
        "{generated}"
    );
    assert!(generated.contains("get kills(): u32"), "{generated}");
    assert!(generated.contains("stats__kills"), "{generated}");
    assert!(generated.contains(".nested(\"stats\")"), "{generated}");
}

#[test]
fn a_nested_descriptor_round_trips_at_version_six() {
    let parsed = parse(NESTED).unwrap();
    let bytes = emit_descriptor(&parsed.schemas, parsed.root.as_ref());
    assert_eq!(bytes[4], DESCRIPTOR_VERSION);
    let back = parse_descriptor(&bytes).expect("descriptor parses");
    let player = back
        .schemas
        .iter()
        .find(|schema| schema.name == "Player")
        .expect("Player survives");
    assert_eq!(player.fields[1].name, "stats__kills");
    assert_eq!(player.fields[1].nested_path.as_deref(), Some("stats"));
}
