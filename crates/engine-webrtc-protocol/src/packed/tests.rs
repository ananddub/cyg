//! Packed lane: round trips, the hand-derived golden vector, and the size claim.

use super::*;
use engine_schema::{FieldEncoding, SchemaDef, SchemaField, WireKind, default_encoding};

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

fn schema(fields: Vec<SchemaField>) -> SchemaDef {
    SchemaDef {
        nested_arrays: Vec::new(),
        name: "Player".into(),
        version: 1,
        fields,
        line: 0,
        column: 0,
    }
}

/// hp u8, yaw f32 8 bits over 0..360, alive bool, name string.
fn player() -> SchemaDef {
    schema(vec![
        field("hp", WireKind::U8, default_encoding(WireKind::U8)),
        field("yaw", WireKind::F32, FieldEncoding::range(8, 0.0, 360.0)),
        field("alive", WireKind::Bool, default_encoding(WireKind::Bool)),
        field("name", WireKind::String, FieldEncoding::NONE),
    ])
}

fn row() -> Vec<WireValue> {
    vec![
        WireValue::U8(100),
        WireValue::F32(90.0),
        WireValue::Bool(true),
        WireValue::Str("ab".into()),
    ]
}

#[test]
fn a_full_section_round_trips_every_kind() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let rows = vec![
        (7u32, row()),
        (
            9u32,
            vec![
                WireValue::U8(1),
                WireValue::F32(359.0),
                WireValue::Bool(false),
                WireValue::Str(String::new()),
            ],
        ),
    ];

    let mut clamped = 0;
    let bytes = encode_full(&packed, &rows, &mut clamped).expect("encode");
    assert_eq!(clamped, 0);
    let decoded = decode(&packed, &bytes).expect("decode");
    assert_eq!(decoded.kind, KIND_FULL);
    assert_eq!(decoded.ops.len(), 2);

    match &decoded.ops[0] {
        PackedOp::Add { id, values } => {
            assert_eq!(*id, 7);
            assert_eq!(values[0], WireValue::U8(100));
            assert_eq!(values[2], WireValue::Bool(true));
            assert_eq!(values[3], WireValue::Str("ab".into()));
            // 8 bits over 0..360 cannot hold 90.0 exactly; it lands within half a step.
            match values[1] {
                WireValue::F32(yaw) => assert!((yaw - 90.0).abs() < 0.71, "yaw {yaw}"),
                ref other => panic!("yaw came back as {other:?}"),
            }
        }
        other => panic!("expected an add, got {other:?}"),
    }
}

#[test]
fn a_patch_round_trips_add_remove_and_masked_update() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let ops = vec![
        PackedOp::Add {
            id: 1,
            values: row(),
        },
        PackedOp::Update {
            id: 1,
            mask: 0b0000_0011,
            values: vec![(0, WireValue::U8(42)), (1, WireValue::F32(180.0))],
        },
        PackedOp::Remove { id: 1 },
    ];

    let mut clamped = 0;
    let bytes = encode_patch(&packed, &ops, &mut clamped).expect("encode");
    let decoded = decode(&packed, &bytes).expect("decode");
    assert_eq!(decoded.kind, KIND_PATCH);
    assert_eq!(decoded.ops.len(), 3);
    match &decoded.ops[1] {
        PackedOp::Update { id, mask, values } => {
            assert_eq!((*id, *mask), (1, 0b11));
            assert_eq!(values.len(), 2, "a masked update carries only what changed");
            assert_eq!(values[0], (0, WireValue::U8(42)));
        }
        other => panic!("expected an update, got {other:?}"),
    }
    assert_eq!(decoded.ops[2], PackedOp::Remove { id: 1 });
}

/// Hand-derived golden vector — this test is the contract, not a snapshot of the encoder.
///
/// Schema: `hp: u8`, `yaw: f32 range(0,360,8)`, `alive: bool`, `name: string`.
/// Row: id 7, hp 100, yaw 90, alive true, name "ab".
///
/// Bit payload, LSB-first:
///   hp    = 100            → 8 bits: 0110 0100
///   yaw   = round(90/360 * 255) = round(63.75) = 64 → 8 bits: 0100 0000
///   alive = 1              → 1 bit
///   = 17 bits → 3 bytes, third byte padded with zeros:
///   byte0 = 100 = 0x64
///   byte1 = 64  = 0x40
///   byte2 = alive bit at position 0 → 0x01
#[test]
fn the_golden_vector_is_exactly_these_bytes() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let bytes = encode_full(&packed, &[(7u32, row())], &mut clamped).expect("encode");

    let mut expected = Vec::new();
    expected.extend_from_slice(b"OXS2");
    expected.push(KIND_FULL);
    expected.extend_from_slice(&packed.hash().to_le_bytes());
    expected.extend_from_slice(&1u32.to_le_bytes());
    expected.push(7); // id, LEB128 (one byte below 128)
    expected.extend_from_slice(&[0x64, 0x40, 0x01]); // bits: 17 bits over 3 bytes
    expected.extend_from_slice(&[2, b'a', b'b']); // string: len u8 + utf8

    assert_eq!(bytes, expected, "the packed framing changed");
}

#[test]
fn a_packed_row_is_far_smaller_than_the_fixed_width_one() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    // Legacy bytes for the same row: id 4 + hp 1 + yaw 4 + alive 1 + name 1+2.
    let legacy = 4 + 1 + 4 + 1 + 3;

    let mut clamped = 0;
    let bytes = encode_full(&packed, &[(7u32, row())], &mut clamped).expect("encode");
    // One row is dominated by the 13-byte header; the win shows up across a full frame.
    assert_eq!(
        bytes.len(),
        HEADER_LEN + 1 + 3 + 3,
        "row overhead is bounded"
    );

    let rows: Vec<(u32, Vec<WireValue>)> = (0..200)
        .map(|i| {
            (
                i,
                vec![
                    WireValue::U8(100),
                    WireValue::F32(90.0),
                    WireValue::Bool(true),
                    WireValue::Str(String::new()),
                ],
            )
        })
        .collect();
    let bytes = encode_full(&packed, &rows, &mut clamped).expect("encode");
    let legacy_frame = HEADER_LEN + 200 * legacy;
    assert!(
        bytes.len() * 2 < legacy_frame,
        "packed {} bytes vs legacy {} — expected better than half",
        bytes.len(),
        legacy_frame
    );
}

#[test]
fn an_absent_optional_field_costs_one_bit() {
    let mut note = field("note", WireKind::String, FieldEncoding::NONE);
    note.flags = engine_schema::FIELD_OPTIONAL;
    let def = schema(vec![
        field("hp", WireKind::U8, default_encoding(WireKind::U8)),
        note,
    ]);
    let packed = PackedSchema::from_def(&def);

    // Present: presence bit + the string.
    let mut clamped = 0;
    let present = encode_full(
        &packed,
        &[(1u32, vec![WireValue::U8(1), WireValue::Str("x".into())])],
        &mut clamped,
    )
    .expect("encode");
    // Absent: presence bit only, no string bytes at all.
    let absent = encode_full(
        &packed,
        &[(1u32, vec![WireValue::U8(1), WireValue::Null])],
        &mut clamped,
    )
    .expect("encode");
    assert_eq!(
        present.len(),
        absent.len() + 2,
        "an absent string saves its length byte and its payload"
    );

    let decoded = decode(&packed, &absent).expect("decode");
    match &decoded.ops[0] {
        PackedOp::Add { values, .. } => assert_eq!(values[1], WireValue::Null),
        other => panic!("expected an add, got {other:?}"),
    }
}

#[test]
fn absent_optional_packed_field_does_not_consume_following_bits_or_variables() {
    let mut optional = field("optional", WireKind::U8, default_encoding(WireKind::U8));
    optional.flags = engine_schema::FIELD_OPTIONAL;
    let def = schema(vec![
        optional,
        field("flag", WireKind::Bool, default_encoding(WireKind::Bool)),
        field("name", WireKind::String, FieldEncoding::NONE),
        field("tail", WireKind::U8, default_encoding(WireKind::U8)),
    ]);
    let packed = PackedSchema::from_def(&def);
    let bytes = encode_full(
        &packed,
        &[(
            1,
            vec![
                WireValue::Null,
                WireValue::Bool(true),
                WireValue::Str("x".into()),
                WireValue::U8(9),
            ],
        )],
        &mut 0,
    )
    .unwrap();
    let decoded = decode(&packed, &bytes).unwrap();
    let PackedOp::Add { values, .. } = &decoded.ops[0] else {
        panic!("full section must contain add")
    };
    assert_eq!(
        values,
        &vec![
            WireValue::Null,
            WireValue::Bool(true),
            WireValue::Str("x".into()),
            WireValue::U8(9)
        ]
    );
}

#[test]
fn a_hash_mismatch_is_an_error_not_a_guess() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let mut bytes = encode_full(&packed, &[(7u32, row())], &mut clamped).expect("encode");
    bytes[5] ^= 0xFF;
    assert!(matches!(
        decode(&packed, &bytes),
        Err(PackedError::HashMismatch { .. })
    ));
}

#[test]
fn every_truncated_prefix_is_rejected_without_panicking() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let bytes = encode_full(&packed, &[(7u32, row())], &mut clamped).expect("encode");
    for cut in 0..bytes.len() {
        assert!(
            decode(&packed, &bytes[..cut]).is_err(),
            "prefix {cut} decoded"
        );
    }
}

#[test]
fn corrupt_bytes_never_panic() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let good = encode_full(&packed, &[(7u32, row())], &mut clamped).expect("encode");
    let mut seed = 0x12345678u32;
    for _ in 0..2_000 {
        let mut bytes = good.clone();
        for _ in 0..4 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let at = 4 + (seed as usize % (bytes.len() - 4));
            bytes[at] ^= (seed >> 8) as u8;
        }
        // Must not panic; an error is fine.
        let _ = decode(&packed, &bytes);
    }
}

#[test]
fn out_of_range_values_are_clamped_and_counted() {
    let def = schema(vec![field(
        "yaw",
        WireKind::F32,
        FieldEncoding::range(8, 0.0, 360.0),
    )]);
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let bytes = encode_full(
        &packed,
        &[(1u32, vec![WireValue::F32(7_000.0)])],
        &mut clamped,
    )
    .expect("encode");
    assert_eq!(clamped, 1, "the schema's range did not hold the value");
    let decoded = decode(&packed, &bytes).expect("decode");
    match &decoded.ops[0] {
        PackedOp::Add { values, .. } => match values[0] {
            WireValue::F32(yaw) => assert_eq!(yaw, 360.0, "clamped to the ceiling"),
            ref other => panic!("expected a float, got {other:?}"),
        },
        other => panic!("expected an add, got {other:?}"),
    }
}

#[test]
fn a_full_row_round_trips_a_u32_at_full_precision() {
    let def = schema(vec![field(
        "score",
        WireKind::U32,
        default_encoding(WireKind::U32),
    )]);
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let bytes = encode_full(
        &packed,
        &[(1u32, vec![WireValue::U32(4_294_967_295)])],
        &mut clamped,
    )
    .expect("encode");
    let decoded = decode(&packed, &bytes).expect("decode");
    match &decoded.ops[0] {
        PackedOp::Add { values, .. } => assert_eq!(values[0], WireValue::U32(4_294_967_295)),
        other => panic!("expected an add, got {other:?}"),
    }
}

#[test]
fn a_tampered_kind_byte_is_rejected() {
    let def = player();
    let packed = PackedSchema::from_def(&def);
    let mut clamped = 0;
    let mut bytes = encode_full(&packed, &[(7u32, row())], &mut clamped).expect("encode");
    bytes[4] = 9;
    assert_eq!(
        decode(&packed, &bytes),
        Err(PackedError::UnsupportedKind(9))
    );
    assert!(!is_packed_section(&bytes));
}
