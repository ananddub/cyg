//! Host pass-through and per-view masking for canonical packed guest sections.

use super::super::{ViewIntent, ViewMask};
use super::*;
use engine_schema::{
    FieldEncoding, SchemaField, WireKind, default_encoding, emit_descriptor, wire_hash,
};
use engine_webrtc_protocol::packed::{self, PackedOp, PackedSchema, WireValue};

fn field(name: &str, kind: WireKind, encoding: FieldEncoding) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: String::new(),
        default: None,
        encoding,
        nested_path: None,
        line: 0,
        column: 0,
    }
}
fn schema_def() -> engine_schema::SchemaDef {
    engine_schema::SchemaDef {
        nested_arrays: Vec::new(),
        name: "Player".into(),
        version: 1,
        fields: vec![
            field("hp", WireKind::U8, default_encoding(WireKind::U8)),
            field("yaw", WireKind::F32, FieldEncoding::range(8, 0.0, 360.0)),
            field("alive", WireKind::Bool, default_encoding(WireKind::Bool)),
            field("name", WireKind::String, FieldEncoding::NONE),
        ],
        line: 0,
        column: 0,
    }
}
fn marker_def() -> engine_schema::SchemaDef {
    engine_schema::SchemaDef {
        nested_arrays: Vec::new(),
        name: "Marker".into(),
        version: 1,
        fields: vec![
            field(
                "x",
                WireKind::F32,
                FieldEncoding::range(16, -1000.0, 1000.0),
            ),
            field("on", WireKind::Bool, default_encoding(WireKind::Bool)),
        ],
        line: 0,
        column: 0,
    }
}
fn root_state() -> engine_schema::RootState {
    engine_schema::RootState {
        nested: Vec::new(),
        name: "Arena".into(),
        version: 1,
        scalars: vec![field(
            "tick",
            WireKind::U16,
            default_encoding(WireKind::U16),
        )],
        collections: vec![
            engine_schema::RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            },
            engine_schema::RootCollection {
                name: "markers".into(),
                schema: "Marker".into(),
            },
        ],
    }
}
fn root_schema() -> engine_schema::SchemaDef {
    engine_schema::SchemaDef {
        nested_arrays: Vec::new(),
        name: "Arena".into(),
        version: 1,
        fields: root_state().scalars,
        line: 0,
        column: 0,
    }
}
fn descriptor_bytes() -> Vec<u8> {
    emit_descriptor(&[schema_def(), marker_def()], Some(&root_state()))
}
fn player_full(rows: &[(u32, u8, f32, bool, &str)]) -> Vec<u8> {
    let values = rows
        .iter()
        .map(|(id, hp, yaw, alive, name)| {
            (
                *id,
                vec![
                    WireValue::U8(*hp),
                    WireValue::F32(*yaw),
                    WireValue::Bool(*alive),
                    WireValue::Str((*name).into()),
                ],
            )
        })
        .collect::<Vec<_>>();
    packed::encode_full(&PackedSchema::from_def(&schema_def()), &values, &mut 0).unwrap()
}
fn player_patch(ops: Vec<PackedOp>) -> Vec<u8> {
    packed::encode_patch(&PackedSchema::from_def(&schema_def()), &ops, &mut 0).unwrap()
}
fn world_full(tick: u16, collections: &[(u16, u32, Vec<u8>)]) -> Vec<u8> {
    packed::encode_world_full(
        &PackedSchema::from_def(&root_schema()),
        &[WireValue::U16(tick)],
        collections,
        &mut 0,
    )
    .unwrap()
}

fn world_patch(tick: Option<u16>, collections: &[(u16, u32, Vec<u8>)]) -> Vec<u8> {
    let mut root_mask = 0u32;
    let mut values = Vec::new();
    if let Some(tick) = tick {
        root_mask |= 1;
        values.push((0, WireValue::U16(tick)));
    }
    packed::encode_world_patch(
        &PackedSchema::from_def(&root_schema()),
        root_mask,
        &values,
        collections,
        &mut 0,
    )
    .unwrap()
}

fn hide(collection: &str, field: &str) -> ViewIntent {
    ViewIntent::HideField {
        seat: 7,
        collection: collection.into(),
        field: field.into(),
    }
}

fn unreliable_schema_def() -> engine_schema::SchemaDef {
    let mut schema = schema_def();
    schema.fields[0].flags |= engine_schema::FIELD_UNRELIABLE;
    schema
}

#[test]
fn unreliable_fields_leave_the_reliable_patch_and_ride_the_volatile_one() {
    let descriptor = emit_descriptor(&[unreliable_schema_def()], None);
    let mut lane = PackedSet::from_descriptor(&descriptor).unwrap();
    let packed_schema = PackedSchema::from_def(&unreliable_schema_def());
    let full = packed::encode_full(
        &packed_schema,
        &[(
            7,
            vec![
                WireValue::U8(100),
                WireValue::F32(90.0),
                WireValue::Bool(true),
                WireValue::Str("ana".into()),
            ],
        )],
        &mut 0,
    )
    .unwrap();
    lane.full(&full).unwrap();
    let patch = packed::encode_patch(
        &packed_schema,
        &[PackedOp::Update {
            id: 7,
            mask: 3,
            values: vec![(0, WireValue::U8(55)), (1, WireValue::F32(180.0))],
        }],
        &mut 0,
    )
    .unwrap();
    let reliable = lane.patch(&patch).unwrap();
    let decoded = packed::decode(&packed_schema, &reliable).unwrap();
    let PackedOp::Update { id, mask, values } = &decoded.ops[0] else {
        panic!("expected update");
    };
    assert_eq!(*id, 7);
    assert_eq!(*mask, 2, "hp is volatile");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].0, 1);
    assert!(matches!(values[0].1, WireValue::F32(v) if (v - 180.0).abs() < 1.0));

    let volatile = lane.volatile_patch().expect("volatile patch");
    let decoded = packed::decode(&packed_schema, &volatile).unwrap();
    let PackedOp::Update { mask, values, .. } = &decoded.ops[0] else {
        panic!("expected volatile update");
    };
    assert_eq!(*mask, 1);
    assert_eq!(values, &vec![(0, WireValue::U8(55))]);
}

#[test]
fn a_private_volatile_field_is_masked_per_viewer() {
    let mut schema = unreliable_schema_def();
    schema.fields[0].flags |= engine_schema::FIELD_PRIVATE;
    let descriptor = emit_descriptor(&[schema.clone()], None);
    let mut lane = PackedSet::from_descriptor(&descriptor).unwrap();
    let packed_schema = PackedSchema::from_def(&schema);
    let full = packed::encode_full(
        &packed_schema,
        &[
            (
                7,
                vec![
                    WireValue::U8(100),
                    WireValue::F32(90.0),
                    WireValue::Bool(true),
                    WireValue::Str("ana".into()),
                ],
            ),
            (
                9,
                vec![
                    WireValue::U8(200),
                    WireValue::F32(180.0),
                    WireValue::Bool(true),
                    WireValue::Str("bob".into()),
                ],
            ),
        ],
        &mut 0,
    )
    .unwrap();
    lane.full(&full).unwrap();

    let mine = lane.volatile_patch_view(7, None).expect("row 7");
    let decoded = packed::decode(&packed_schema, &mine).unwrap();
    assert_eq!(decoded.ops.len(), 1);
    let PackedOp::Update { id, values, .. } = &decoded.ops[0] else {
        panic!("expected update");
    };
    assert_eq!(*id, 7);
    assert_eq!(values, &vec![(0, WireValue::U8(100))]);

    let theirs = lane.volatile_patch_view(9, None).expect("row 9");
    let decoded = packed::decode(&packed_schema, &theirs).unwrap();
    assert_eq!(decoded.ops.len(), 1);
    assert!(matches!(
        &decoded.ops[0],
        PackedOp::Update { id: 9, values, .. } if values == &vec![(0, WireValue::U8(200))]
    ));

    assert!(lane.volatile_patch_view(8, None).is_none());
}

#[test]
fn world_root_unreliable_fields_are_split_too() {
    let mut root = root_state();
    root.scalars[0].flags |= engine_schema::FIELD_UNRELIABLE;
    let root_def = engine_schema::SchemaDef {
        nested_arrays: Vec::new(),
        name: root.name.clone(),
        version: root.version,
        fields: root.scalars.clone(),
        line: 0,
        column: 0,
    };
    let descriptor = emit_descriptor(&[schema_def(), marker_def()], Some(&root));
    let mut lane = PackedSet::from_descriptor(&descriptor).unwrap();
    let packed_root = PackedSchema::from_def(&root_def);
    let full = packed::encode_world_full(&packed_root, &[WireValue::U16(42)], &[], &mut 0).unwrap();
    lane.full(&full).unwrap();

    let patch =
        packed::encode_world_patch(&packed_root, 1, &[(0, WireValue::U16(43))], &[], &mut 0)
            .unwrap();
    let reliable = lane.patch(&patch).unwrap();
    let decoded = packed::decode_world(&packed_root, &reliable).unwrap();
    assert_eq!(decoded.root_mask, 0);
    assert!(decoded.root.is_empty());

    let volatile = lane.volatile_patch().expect("volatile world patch");
    let decoded = packed::decode_world(&packed_root, &volatile).unwrap();
    assert_eq!(decoded.root_mask, 1);
    assert_eq!(decoded.root, vec![(0, WireValue::U16(43))]);
}

#[test]
fn packed_guest_full_passes_through_byte_for_byte() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).unwrap();
    let input = player_full(&[(7, 100, 90.0, true, "ana")]);
    assert_eq!(lane.full(&input).unwrap(), input);
    let decoded = packed::decode(&PackedSchema::from_def(&schema_def()), &input).unwrap();
    assert_eq!(decoded.kind, packed::KIND_FULL);
    assert!(
        matches!(&decoded.ops[0], PackedOp::Add { id: 7, values } if values[0] == WireValue::U8(100) && values[2] == WireValue::Bool(true))
    );
}

#[test]
fn packed_guest_patch_passes_through_and_decodes() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).unwrap();
    let input = player_patch(vec![
        PackedOp::Update {
            id: 7,
            mask: 3,
            values: vec![(0, WireValue::U8(55)), (1, WireValue::F32(180.0))],
        },
        PackedOp::Remove { id: 9 },
    ]);
    assert_eq!(lane.patch(&input).unwrap(), input);
}

#[test]
fn packed_world_frames_pass_through_and_decode() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).unwrap();
    let players = player_full(&[(7, 100, 90.0, true, "ana")]);
    let markers = packed::encode_full(
        &PackedSchema::from_def(&marker_def()),
        &[(3, vec![WireValue::F32(12.5), WireValue::Bool(true)])],
        &mut 0,
    )
    .unwrap();
    let input = world_full(
        42,
        &[
            (0, wire_hash(&schema_def()), players),
            (1, wire_hash(&marker_def()), markers),
        ],
    );
    assert_eq!(lane.full(&input).unwrap(), input);
    let decoded = packed::decode_world(&PackedSchema::from_def(&root_schema()), &input).unwrap();
    assert_eq!(decoded.kind, packed::KIND_WORLD_FULL);
    assert_eq!(decoded.root, vec![(0, WireValue::U16(42))]);
}

#[test]
fn fixed_width_guest_kinds_are_rejected_with_rebuild_error() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).unwrap();
    for kind in 1..=4 {
        let mut bytes = b"OXS2".to_vec();
        bytes.push(kind);
        bytes.extend_from_slice(&[0; 16]);
        let err = lane.full(&bytes).unwrap_err();
        assert!(err.contains("rebuild the room wasm"), "{err}");
    }
}

fn private_schema() -> engine_schema::SchemaDef {
    let mut schema = schema_def();
    schema.fields[0].flags |= engine_schema::FIELD_PRIVATE;
    schema
}
fn private_full() -> Vec<u8> {
    let schema = private_schema();
    packed::encode_full(
        &PackedSchema::from_def(&schema),
        &[
            (
                7,
                vec![
                    WireValue::U8(100),
                    WireValue::F32(90.0),
                    WireValue::Bool(true),
                    WireValue::Str("ana".into()),
                ],
            ),
            (
                9,
                vec![
                    WireValue::U8(1),
                    WireValue::F32(0.0),
                    WireValue::Bool(true),
                    WireValue::Str("bo".into()),
                ],
            ),
        ],
        &mut 0,
    )
    .unwrap()
}

#[test]
fn private_fields_are_masked_only_for_nonowners() {
    let schema = private_schema();
    let mut lane = PackedSet::from_descriptor(&emit_descriptor(&[schema.clone()], None)).unwrap();
    let bytes = private_full();
    let owner = packed::decode(
        &PackedSchema::from_def(&schema),
        &lane.full_for(&bytes, 7).unwrap(),
    )
    .unwrap();
    assert!(
        matches!(&owner.ops[0], PackedOp::Add { values, .. } if values[0] == WireValue::U8(100))
    );
    assert!(matches!(&owner.ops[1], PackedOp::Add { values, .. } if values[0] == WireValue::U8(0)));
    let other = packed::decode(
        &PackedSchema::from_def(&schema),
        &lane.full_for(&bytes, 9).unwrap(),
    )
    .unwrap();
    assert!(matches!(&other.ops[0], PackedOp::Add { values, .. } if values[0] == WireValue::U8(0)));
    assert!(matches!(&other.ops[1], PackedOp::Add { values, .. } if values[0] == WireValue::U8(1)));
}

#[test]
fn view_mask_hides_field_and_only_self_filters_rows() {
    let schema = schema_def();
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).unwrap();
    let mut view = ViewMask::default();
    lane.apply_view_intent(&mut view, &hide("players", "hp"))
        .unwrap();
    lane.apply_view_intent(
        &mut view,
        &ViewIntent::OnlySelf {
            seat: 7,
            enabled: true,
        },
    )
    .unwrap();
    let bytes = player_full(&[(7, 100, 90.0, true, "ana"), (9, 1, 0.0, true, "bo")]);
    let decoded = packed::decode(
        &PackedSchema::from_def(&schema),
        &lane.full_for_view(&bytes, 7, &view).unwrap(),
    )
    .unwrap();
    assert_eq!(decoded.ops.len(), 1);
    assert!(
        matches!(&decoded.ops[0], PackedOp::Add { id: 7, values } if values[0] == WireValue::U8(0))
    );
}

#[test]
fn world_view_masks_root_and_nested_collection_data() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).unwrap();
    let mut view = ViewMask::default();
    lane.apply_view_intent(&mut view, &hide("__root__", "tick"))
        .unwrap();
    let input = world_full(
        42,
        &[(
            0,
            wire_hash(&schema_def()),
            player_full(&[(7, 100, 90.0, true, "ana")]),
        )],
    );
    let output = lane.full_for_view(&input, 7, &view).unwrap();
    let decoded = packed::decode_world(&PackedSchema::from_def(&root_schema()), &output).unwrap();
    assert_ne!(decoded.root[0].1, WireValue::U16(42));
}

#[test]
fn a_per_view_budget_defers_low_priority_updates_and_retries_them() {
    let descriptor = emit_descriptor(&[schema_def(), marker_def()], Some(&root_state()));
    let mut lane = PackedSet::from_descriptor(&descriptor).unwrap();
    let a = PackedOp::Update {
        id: 1,
        mask: 1,
        values: vec![(0, WireValue::U8(10))],
    };
    let b = PackedOp::Update {
        id: 2,
        mask: 1,
        values: vec![(0, WireValue::U8(20))],
    };
    let one = packed::encode_patch(
        &PackedSchema::from_def(&schema_def()),
        std::slice::from_ref(&a),
        &mut 0,
    )
    .unwrap()
    .len();
    let patch = player_patch(vec![a, b]);

    let first = lane.patch_for_budget("arena", &patch, 7, one).unwrap();
    let decoded = packed::decode(&PackedSchema::from_def(&schema_def()), &first).unwrap();
    assert_eq!(decoded.ops.len(), 1, "budget carried only the first update");
    assert_eq!(op_id(&decoded.ops[0]), 1);

    // The next patch is empty, but the deferred update must still ride it.
    let empty = player_patch(Vec::new());
    let second = lane.patch_for_budget("arena", &empty, 7, one).unwrap();
    let decoded = packed::decode(&PackedSchema::from_def(&schema_def()), &second).unwrap();
    assert_eq!(decoded.ops.len(), 1);
    assert_eq!(op_id(&decoded.ops[0]), 2);

    let third = lane.patch_for_budget("arena", &empty, 7, one).unwrap();
    let decoded = packed::decode(&PackedSchema::from_def(&schema_def()), &third).unwrap();
    assert!(decoded.ops.is_empty(), "deferred queue drained");
}

#[test]
fn a_new_update_for_the_same_row_supersedes_a_deferred_one() {
    let descriptor = emit_descriptor(&[schema_def(), marker_def()], Some(&root_state()));
    let mut lane = PackedSet::from_descriptor(&descriptor).unwrap();
    let old = PackedOp::Update {
        id: 3,
        mask: 1,
        values: vec![(0, WireValue::U8(1))],
    };
    let new = PackedOp::Update {
        id: 3,
        mask: 1,
        values: vec![(0, WireValue::U8(2))],
    };
    let one = packed::encode_patch(
        &PackedSchema::from_def(&schema_def()),
        std::slice::from_ref(&old),
        &mut 0,
    )
    .unwrap()
    .len();
    lane.patch_for_budget("arena", &player_patch(vec![old]), 7, 1)
        .unwrap();
    let out = lane
        .patch_for_budget("arena", &player_patch(vec![new]), 7, one)
        .unwrap();
    let decoded = packed::decode(&PackedSchema::from_def(&schema_def()), &out).unwrap();
    assert_eq!(decoded.ops.len(), 1);
    let PackedOp::Update { values, .. } = &decoded.ops[0] else {
        panic!("expected update");
    };
    assert_eq!(values, &vec![(0, WireValue::U8(2))], "fresh value wins");
}

#[test]
fn a_world_patch_budget_defers_collection_updates_too() {
    let descriptor = emit_descriptor(&[schema_def(), marker_def()], Some(&root_state()));
    let mut lane = PackedSet::from_descriptor(&descriptor).unwrap();
    let a = PackedOp::Update {
        id: 1,
        mask: 1,
        values: vec![(0, WireValue::U8(10))],
    };
    let b = PackedOp::Update {
        id: 2,
        mask: 1,
        values: vec![(0, WireValue::U8(20))],
    };
    let one = player_patch(vec![a.clone()]).len();
    let hash = wire_hash(&schema_def());
    let patch = world_patch(Some(9), &[(0, hash, player_patch(vec![a, b]))]);
    let budget = one + 18;
    let first = lane.patch_for_budget("arena", &patch, 7, budget).unwrap();
    let decoded = packed::decode_world(&PackedSchema::from_def(&root_schema()), &first).unwrap();
    let inner = packed::decode(
        &PackedSchema::from_def(&schema_def()),
        &decoded.collections[0].2,
    )
    .unwrap();
    assert_eq!(inner.ops.len(), 1, "world budget carried one update");
    assert_eq!(op_id(&inner.ops[0]), 1);

    let empty = world_patch(None, &[(0, hash, player_patch(Vec::new()))]);
    let second = lane.patch_for_budget("arena", &empty, 7, budget).unwrap();
    let decoded = packed::decode_world(&PackedSchema::from_def(&root_schema()), &second).unwrap();
    let inner = packed::decode(
        &PackedSchema::from_def(&schema_def()),
        &decoded.collections[0].2,
    )
    .unwrap();
    assert_eq!(inner.ops.len(), 1, "deferred world update retried");
    assert_eq!(op_id(&inner.ops[0]), 2);
}

#[test]
fn adaptive_budget_uses_pacing_and_keeps_a_floor_and_ceiling() {
    let tick = std::time::Duration::from_millis(33);
    assert_eq!(adaptive_patch_budget(0.0, tick), DEFAULT_PATCH_BUDGET);
    let slow = adaptive_patch_budget(20_000.0, tick);
    let fast = adaptive_patch_budget(5_000_000.0, tick);
    assert_eq!(slow, MIN_PATCH_BUDGET);
    assert_eq!(fast, MAX_PATCH_BUDGET);
    assert!(slow < fast);
}
