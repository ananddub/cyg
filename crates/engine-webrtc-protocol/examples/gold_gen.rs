//! One-off: prints the golden vector for tests/packed_golden.json.
use engine_schema::{FieldEncoding, SchemaDef, SchemaField, WireKind, default_encoding, wire_hash};
use engine_webrtc_protocol::packed::{self, PackedOp, PackedSchema, WireValue};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let f = |name: &str, kind: WireKind, enc: FieldEncoding| SchemaField {
        name: name.into(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: String::new(),
        default: None,
        encoding: enc,
        line: 0,
        column: 0,
    };
    let def = SchemaDef {
        name: "V".into(),
        version: 1,
        line: 0,
        column: 0,
        fields: vec![
            f("hp", WireKind::U8, default_encoding(WireKind::U8)),
            f("yaw", WireKind::F32, FieldEncoding::range(8, 0.0, 360.0)),
            f("alive", WireKind::Bool, default_encoding(WireKind::Bool)),
            f("name", WireKind::String, FieldEncoding::NONE),
            f("score", WireKind::U32, default_encoding(WireKind::U32)),
        ],
    };
    let packed = PackedSchema::from_def(&def);
    println!("hash {:#010x}", wire_hash(&def));
    let rows = vec![(
        7u32,
        vec![
            WireValue::U8(100),
            WireValue::F32(90.0),
            WireValue::Bool(true),
            WireValue::Str("ab".into()),
            WireValue::U32(42),
        ],
    )];
    let mut clamped = 0;
    println!(
        "full {}",
        hex(&packed::encode_full(&packed, &rows, &mut clamped).unwrap())
    );
    let ops = vec![
        PackedOp::Update {
            id: 7,
            mask: 0b11,
            values: vec![(0, WireValue::U8(55)), (1, WireValue::F32(180.0))],
        },
        PackedOp::Remove { id: 7 },
    ];
    println!(
        "patch {}",
        hex(&packed::encode_patch(&packed, &ops, &mut clamped).unwrap())
    );
}
