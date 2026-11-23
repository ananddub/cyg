use crate::ts::parse;
use crate::{DESCRIPTOR_VERSION, emit_descriptor, parse_descriptor};

const NESTED_ARRAY: &str = r#"
    const Item = schema({
        kind: t.uint8(),
        qty: t.uint16(),
    }, "Item");

    const Player = schema({
        hp: t.uint16(),
        items: t.array(Item),
    }, "Player");

    const State = schema({
        players: t.map(Player),
    }, "State");
"#;

#[test]
fn a_nested_array_lowers_to_a_child_collection() {
    let parsed = parse(NESTED_ARRAY).expect("nested array parses");
    let root = parsed.root.as_ref().expect("root state");
    let names: Vec<&str> = root.collections.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["players", "players_items"]);
    assert_eq!(root.nested.len(), 1);
    assert_eq!(root.nested[0].collection, "players_items");
    assert_eq!(root.nested[0].parent, "players");
    assert_eq!(root.nested[0].field, "items");
    assert_eq!(root.nested[0].schema, "Item");
}

#[test]
fn nested_array_codegen_syncs_child_rows_before_commit() {
    let parsed = parse(NESTED_ARRAY).unwrap();
    let generated = crate::emit_server(&parsed.schemas, parsed.root.as_ref(), "nested.ts");
    assert!(
        generated.contains("items: Array<Item> = new Array<Item>();"),
        "{generated}"
    );
    assert!(
        generated.contains("players_items: SchemaState;"),
        "{generated}"
    );
    assert!(generated.contains("_sync_players_items"), "{generated}");
    assert!(generated.contains("override commit(): i64"), "{generated}");
    assert!(
        generated.contains("(ownerId << 16) | (slot as u32)"),
        "{generated}"
    );
    let dts = crate::emit_client_dts(&parsed.schemas, parsed.root.as_ref(), "nested.ts");
    assert!(dts.contains("items: ItemState[];"), "{dts}");
    assert!(
        !dts.contains("players_items: SchemaMap"),
        "child collection stays hidden: {dts}"
    );
}

#[test]
fn a_nested_array_descriptor_round_trips() {
    let parsed = parse(NESTED_ARRAY).unwrap();
    let bytes = emit_descriptor(&parsed.schemas, parsed.root.as_ref());
    assert_eq!(bytes[4], DESCRIPTOR_VERSION);
    let back = parse_descriptor(&bytes).expect("descriptor parses");
    assert_eq!(back.root.nested.len(), 1);
    assert_eq!(back.root.nested[0].collection, "players_items");
    assert_eq!(back.root.nested[0].parent, "players");
    assert_eq!(back.root.nested[0].field, "items");
}

#[test]
fn nested_arrays_can_nest_one_more_level() {
    let source = r#"
        const Part = schema({ hp: t.uint8() }, "Part");
        const Item = schema({ qty: t.uint16(), parts: t.array(Part) }, "Item");
        const Player = schema({ items: t.array(Item) }, "Player");
        const State = schema({ players: t.map(Player) }, "State");
    "#;
    let parsed = parse(source).unwrap();
    let root = parsed.root.as_ref().unwrap();
    let names: Vec<&str> = root.collections.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["players", "players_items", "players_items_parts"]
    );
    assert_eq!(root.nested.len(), 2);
    assert!(
        root.nested
            .iter()
            .any(|n| n.parent == "players_items" && n.field == "parts")
    );
}
