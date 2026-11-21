//! Transcoding: the packed frame the host builds must decode to the values the guest sent.

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
        line: 0,
        column: 0,
    }
}

fn schema_def() -> engine_schema::SchemaDef {
    engine_schema::SchemaDef {
        name: "Player".into(),
        version: 1,
        fields: vec![
            field("hp", WireKind::U8, default_encoding(WireKind::U8)),
            // 8 bits over a full turn: the kind of field that makes packing worth it.
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
        name: "Marker".into(),
        version: 1,
        fields: vec![
            field("x", WireKind::F32, FieldEncoding::range(16, -1000.0, 1000.0)),
            field("on", WireKind::Bool, default_encoding(WireKind::Bool)),
        ],
        line: 0,
        column: 0,
    }
}

fn root_state() -> engine_schema::RootState {
    engine_schema::RootState {
        name: "Arena".into(),
        version: 1,
        scalars: vec![field("tick", WireKind::U16, default_encoding(WireKind::U16))],
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

fn legacy_full(rows: &[(u32, u8, f32, bool, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"OXS2");
    out.push(1);
    out.extend_from_slice(&wire_hash(&schema_def()).to_le_bytes());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for (id, hp, yaw, alive, name) in rows {
        out.extend_from_slice(&id.to_le_bytes());
        out.push(*hp);
        out.extend_from_slice(&yaw.to_le_bytes());
        out.push(u8::from(*alive));
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
    }
    out
}

fn legacy_marker_full(rows: &[(u32, f32, bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"OXS2");
    out.push(1);
    out.extend_from_slice(&wire_hash(&marker_def()).to_le_bytes());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for (id, x, on) in rows {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&x.to_le_bytes());
        out.push(u8::from(*on));
    }
    out
}

fn legacy_world_full(tick: u16, collections: &[(u16, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"OXS2");
    out.push(3);
    out.extend_from_slice(&wire_hash(&root_schema()).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // one root scalar
    out.extend_from_slice(&tick.to_le_bytes());
    out.extend_from_slice(&(collections.len() as u16).to_le_bytes());
    for (id, hash, section) in collections {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&hash.to_le_bytes());
        out.extend_from_slice(&(section.len() as u32).to_le_bytes());
        out.extend_from_slice(section);
    }
    out
}

#[test]
fn a_transcoded_full_frame_carries_the_same_values() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).expect("schemas");
    let legacy = legacy_full(&[(7, 100, 90.0, true, "ana"), (9, 1, 359.0, false, "")]);
    let packed_bytes = lane.full(&legacy).expect("transcodes");

    let decoded = packed::decode(&PackedSchema::from_def(&schema_def()), &packed_bytes)
        .expect("the host's own frame decodes");
    assert_eq!(decoded.kind, packed::KIND_FULL);
    assert_eq!(decoded.ops.len(), 2);
    match &decoded.ops[0] {
        PackedOp::Add { id, values } => {
            assert_eq!(*id, 7);
            assert_eq!(values[0], WireValue::U8(100));
            assert_eq!(values[2], WireValue::Bool(true));
            assert_eq!(values[3], WireValue::Str("ana".into()));
            match values[1] {
                WireValue::F32(yaw) => assert!((yaw - 90.0).abs() < 0.71, "yaw {yaw}"),
                ref other => panic!("yaw came back as {other:?}"),
            }
        }
        other => panic!("expected an add, got {other:?}"),
    }
    assert!(
        packed_bytes.len() < legacy.len(),
        "packed {} vs legacy {}",
        packed_bytes.len(),
        legacy.len()
    );
    assert_eq!(lane.clamped, 0);
}

#[test]
fn a_transcoded_patch_carries_the_same_ops() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).expect("schemas");
    let hash = wire_hash(&schema_def());
    let mut legacy = Vec::new();
    legacy.extend_from_slice(b"OXS2");
    legacy.push(2);
    legacy.extend_from_slice(&hash.to_le_bytes());
    legacy.extend_from_slice(&2u32.to_le_bytes());
    legacy.push(3); // update
    legacy.extend_from_slice(&7u32.to_le_bytes());
    legacy.extend_from_slice(&0b0011u32.to_le_bytes());
    legacy.push(55);
    legacy.extend_from_slice(&180.0f32.to_le_bytes());
    legacy.push(2); // remove
    legacy.extend_from_slice(&9u32.to_le_bytes());

    let packed_bytes = lane.patch(&legacy).expect("transcodes");
    let decoded =
        packed::decode(&PackedSchema::from_def(&schema_def()), &packed_bytes).expect("decodes");
    assert_eq!(decoded.kind, packed::KIND_PATCH);
    assert_eq!(decoded.ops.len(), 2);
    match &decoded.ops[0] {
        PackedOp::Update { id, mask, values } => {
            assert_eq!((*id, *mask), (7, 0b11));
            assert_eq!(values[0], (0, WireValue::U8(55)));
        }
        other => panic!("expected an update, got {other:?}"),
    }
    assert_eq!(decoded.ops[1], PackedOp::Remove { id: 9 });
}

#[test]
fn a_world_frame_packs_root_scalars_and_every_collection() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).expect("schemas");
    let players = legacy_full(&[(7, 100, 90.0, true, "ana")]);
    let markers = legacy_marker_full(&[(3, 12.5, true)]);
    let legacy = legacy_world_full(
        42,
        &[
            (0, wire_hash(&schema_def()), players),
            (1, wire_hash(&marker_def()), markers),
        ],
    );
    let packed_bytes = lane.full(&legacy).expect("transcodes");

    let decoded =
        packed::decode_world(&PackedSchema::from_def(&root_schema()), &packed_bytes).expect("world");
    assert_eq!(decoded.kind, packed::KIND_WORLD_FULL);
    assert_eq!(decoded.root, vec![(0, WireValue::U16(42))]);
    assert_eq!(decoded.collections.len(), 2);
    assert_eq!(decoded.collections[0].0, 0);
    assert_eq!(decoded.collections[1].0, 1);

    let first = packed::decode(
        &PackedSchema::from_def(&schema_def()),
        &decoded.collections[0].2,
    )
    .expect("player section");
    assert_eq!(first.kind, packed::KIND_FULL);
    assert_eq!(first.ops.len(), 1);
    let second = packed::decode(
        &PackedSchema::from_def(&marker_def()),
        &decoded.collections[1].2,
    )
    .expect("marker section");
    assert_eq!(second.ops.len(), 1);
}

#[test]
fn a_world_patch_packs_only_the_dirty_root_fields_and_changed_collections() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).expect("schemas");
    let marker_hash = wire_hash(&marker_def());

    // Root patch: tick dirty. One collection: markers patch with an update.
    let mut marker_patch = Vec::new();
    marker_patch.extend_from_slice(b"OXS2");
    marker_patch.push(2);
    marker_patch.extend_from_slice(&marker_hash.to_le_bytes());
    marker_patch.extend_from_slice(&1u32.to_le_bytes());
    marker_patch.push(3); // UPDATE
    marker_patch.extend_from_slice(&3u32.to_le_bytes());
    marker_patch.extend_from_slice(&0b01u32.to_le_bytes());
    marker_patch.extend_from_slice(&55.5f32.to_le_bytes());

    let mut legacy = Vec::new();
    legacy.extend_from_slice(b"OXS2");
    legacy.push(4);
    legacy.extend_from_slice(&wire_hash(&root_schema()).to_le_bytes());
    legacy.extend_from_slice(&1u16.to_le_bytes());
    legacy.extend_from_slice(&0b1u32.to_le_bytes()); // tick dirty
    legacy.extend_from_slice(&43u16.to_le_bytes());
    legacy.extend_from_slice(&1u16.to_le_bytes()); // one changed collection
    legacy.extend_from_slice(&1u16.to_le_bytes());
    legacy.extend_from_slice(&marker_hash.to_le_bytes());
    legacy.extend_from_slice(&(marker_patch.len() as u32).to_le_bytes());
    legacy.extend_from_slice(&marker_patch);

    let packed_bytes = lane.patch(&legacy).expect("transcodes");
    let decoded =
        packed::decode_world(&PackedSchema::from_def(&root_schema()), &packed_bytes).expect("world");
    assert_eq!(decoded.kind, packed::KIND_WORLD_PATCH);
    assert_eq!(decoded.root_mask, 0b1);
    assert_eq!(decoded.root, vec![(0, WireValue::U16(43))]);
    assert_eq!(decoded.collections.len(), 1);
    assert_eq!(decoded.collections[0].0, 1);
}

#[test]
fn an_out_of_range_value_is_clamped_and_counted_not_dropped() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).expect("schemas");
    // yaw is declared 0..360; the guest sent 7_000.
    let legacy = legacy_full(&[(1, 1, 7_000.0, true, "")]);
    let packed_bytes = lane.full(&legacy).expect("transcodes");
    assert_eq!(
        lane.clamped, 1,
        "yaw beyond its declared range must be counted"
    );
    let decoded =
        packed::decode(&PackedSchema::from_def(&schema_def()), &packed_bytes).expect("decodes");
    match &decoded.ops[0] {
        PackedOp::Add { values, .. } => match values[1] {
            WireValue::F32(yaw) => assert_eq!(yaw, 360.0),
            ref other => panic!("yaw came back as {other:?}"),
        },
        other => panic!("expected an add, got {other:?}"),
    }
}

#[test]
fn a_descriptor_without_schemas_yields_no_lane() {
    let bytes = emit_descriptor(&[], Some(&root_state()));
    assert!(PackedSet::from_descriptor(&bytes).is_none());
    assert!(PackedSet::from_descriptor(b"not a descriptor").is_none());
}

#[test]
fn a_frame_from_a_different_schema_fails_loudly_and_counts() {
    let mut lane = PackedSet::from_descriptor(&descriptor_bytes()).expect("schemas");
    let mut other = schema_def();
    other.version = 9;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"OXS2");
    bytes.push(1);
    bytes.extend_from_slice(&wire_hash(&other).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert!(
        lane.full(&bytes).is_err(),
        "a foreign schema must not transcode"
    );
    assert_eq!(lane.failures, 1);
}
