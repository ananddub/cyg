use cyg_core_rs::ClientCore;
use engine_schema::{
    FieldEncoding, NestedCollection, RootCollection, RootState, SchemaDef, SchemaField, WireKind,
    emit_descriptor, wire_hash,
};
use engine_webrtc_protocol::packed::{self, PackedSchema, WireValue};

fn field(name: &str, kind: WireKind) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: String::new(),
        default: None,
        encoding: engine_schema::default_encoding(kind),
        nested_path: None,
        line: 0,
        column: 0,
    }
}

fn schema(name: &str, fields: Vec<SchemaField>) -> SchemaDef {
    SchemaDef {
        name: name.into(),
        version: 1,
        fields,
        nested_arrays: Vec::new(),
        line: 0,
        column: 0,
    }
}

#[test]
fn a_child_collection_decodes_with_its_owner_and_slot_id() {
    let item = schema("Item", vec![field("qty", WireKind::U16)]);
    let player = schema("Player", vec![field("hp", WireKind::U16)]);
    let root = RootState {
        name: "State".into(),
        version: 1,
        scalars: Vec::new(),
        collections: vec![
            RootCollection { name: "players".into(), schema: "Player".into() },
            RootCollection { name: "players_items".into(), schema: "Item".into() },
        ],
        nested: vec![NestedCollection {
            collection: "players_items".into(),
            parent: "players".into(),
            field: "items".into(),
            schema: "Item".into(),
        }],
    };
    let schemas = vec![item, player];
    let descriptor = emit_descriptor(&schemas, Some(&root));
    let mut core = ClientCore::new(&descriptor).expect("descriptor parses");
    assert_eq!(core.descriptor().root.nested.len(), 1);

    let player_section = packed::encode_full(
        &PackedSchema::from_def(&schemas[1]),
        &[(7, vec![WireValue::U16(100)])],
        &mut 0,
    )
    .unwrap();
    let child_id = (7u32 << 16) | 1;
    let item_section = packed::encode_full(
        &PackedSchema::from_def(&schemas[0]),
        &[(child_id, vec![WireValue::U16(5)])],
        &mut 0,
    )
    .unwrap();
    let root_packed = PackedSchema::from_def(&SchemaDef {
        name: root.name.clone(),
        version: root.version,
        fields: Vec::new(),
        nested_arrays: Vec::new(),
        line: 0,
        column: 0,
    });
    let world = packed::encode_world_full(
        &root_packed,
        &[],
        &[
            (0, wire_hash(&schemas[1]), player_section),
            (1, wire_hash(&schemas[0]), item_section),
        ],
        &mut 0,
    )
    .unwrap();
    core.apply_section_direct(&world).expect("world decodes");

    assert_eq!(core.state().collection("players").unwrap().entities.len(), 1);
    let children = &core.state().collection("players_items").unwrap().entities;
    assert_eq!(children.get(&child_id).unwrap().values[0], cyg_core_rs::state::Value::U16(5));
}
