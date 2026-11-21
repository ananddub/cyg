//! The host's read-back of the guest's legacy section must agree with the client decoders.

use super::*;
use engine_schema::{FieldEncoding, SchemaField, default_encoding, wire_hash};

fn field(name: &str, kind: WireKind, encoding: FieldEncoding, element: Option<WireKind>) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element,
        ref_target: None,
        flags: 0,
        source_type: String::new(),
        default: None,
        encoding,
        line: 0,
        column: 0,
    }
}

/// hp u8, yaw f32, alive bool, name string, path array of f64, flags u16.
fn schema() -> SchemaDef {
    SchemaDef {
        name: "Player".into(),
        version: 1,
        fields: vec![
            field("hp", WireKind::U8, default_encoding(WireKind::U8), None),
            field("yaw", WireKind::F32, FieldEncoding::NONE, None),
            field("alive", WireKind::Bool, default_encoding(WireKind::Bool), None),
            field("name", WireKind::String, FieldEncoding::NONE, None),
            field(
                "path",
                WireKind::Array,
                FieldEncoding::NONE,
                Some(WireKind::F64),
            ),
            field("flags", WireKind::U16, default_encoding(WireKind::U16), None),
        ],
        line: 0,
        column: 0,
    }
}

fn full(rows: &[(u32, u8, f32, bool, &str, &[f64], u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(KIND_FULL);
    out.extend_from_slice(&wire_hash(&schema()).to_le_bytes());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for (id, hp, yaw, alive, name, path, flags) in rows {
        out.extend_from_slice(&id.to_le_bytes());
        out.push(*hp);
        out.extend_from_slice(&yaw.to_le_bytes());
        out.push(u8::from(*alive));
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
        out.push(path.len() as u8);
        for value in path.iter() {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&flags.to_le_bytes());
    }
    out
}

#[test]
fn a_full_section_decodes_to_the_values_it_carries() {
    let bytes = full(&[(7, 100, 90.5, true, "ana", &[1.5, 2.5], 9)]);
    let rows = decode_full(&bytes, &schema()).expect("decodes");
    assert_eq!(rows.len(), 1);
    let (id, values) = &rows[0];
    assert_eq!(*id, 7);
    assert_eq!(values[0], WireValue::U8(100));
    assert_eq!(values[1], WireValue::F32(90.5));
    assert_eq!(values[2], WireValue::Bool(true));
    assert_eq!(values[3], WireValue::Str("ana".into()));
    assert_eq!(
        values[4],
        WireValue::Arr(vec![WireValue::F64(1.5), WireValue::F64(2.5)])
    );
    assert_eq!(values[5], WireValue::U16(9));
}

#[test]
fn a_patch_decodes_into_add_update_and_remove_ops() {
    let hash = wire_hash(&schema());
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.push(KIND_PATCH);
    bytes.extend_from_slice(&hash.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());

    bytes.push(1); // OP_ADD
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.push(10);
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.push(1);
    bytes.push(3);
    bytes.extend_from_slice(b"bob");
    bytes.push(0);
    bytes.extend_from_slice(&5u16.to_le_bytes());

    bytes.push(3); // OP_UPDATE
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&0b0000_0011u32.to_le_bytes()); // hp + yaw
    bytes.push(11);
    bytes.extend_from_slice(&2.0f32.to_le_bytes());

    bytes.push(2); // OP_REMOVE
    bytes.extend_from_slice(&9u32.to_le_bytes());

    let ops = decode_patch(&bytes, &schema()).expect("decodes");
    assert_eq!(ops.len(), 3);
    match &ops[0] {
        PackedOp::Add { id, values } => {
            assert_eq!(*id, 4);
            assert_eq!(values[3], WireValue::Str("bob".into()));
        }
        other => panic!("expected an add, got {other:?}"),
    }
    match &ops[1] {
        PackedOp::Update { id, mask, values } => {
            assert_eq!((*id, *mask), (4, 0b11));
            assert_eq!(values, &vec![(0, WireValue::U8(11)), (1, WireValue::F32(2.0))]);
        }
        other => panic!("expected an update, got {other:?}"),
    }
    assert_eq!(ops[2], PackedOp::Remove { id: 9 });
}

#[test]
fn a_foreign_schema_is_rejected_not_misread() {
    let bytes = full(&[(1, 1, 0.0, false, "", &[], 0)]);
    let mut other = schema();
    other.version = 2; // different wire hash
    let error = decode_full(&bytes, &other).expect_err("must reject");
    assert!(error.contains("hash mismatch"), "{error}");
}

#[test]
fn the_wrong_kind_is_rejected() {
    let bytes = full(&[(1, 1, 0.0, false, "", &[], 0)]);
    let error = decode_patch(&bytes, &schema()).expect_err("must reject a full section");
    assert!(error.contains("patch section"), "{error}");
}

#[test]
fn truncated_and_corrupt_sections_do_not_panic() {
    let bytes = full(&[(1, 1, 0.0, false, "ab", &[1.0], 3)]);
    for cut in 0..bytes.len() {
        assert!(decode_full(&bytes[..cut], &schema()).is_err(), "prefix {cut}");
    }
    // A corrupt row count must not drive a huge loop or allocation.
    let mut corrupt = bytes.clone();
    corrupt[9..13].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_full(&corrupt, &schema()).is_err());
}
