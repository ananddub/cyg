//! Pins the packed bytes to a file every SDK can read.
//!
//! The point is not that "the encoder produces these bytes" — it is that Rust, and a browser
//! decoder written months from now, agree on the *same* bytes. A failure here means one of them
//! would decode garbage in production, which no amount of round-trip tests can catch.

use engine_schema::{FieldEncoding, SchemaDef, SchemaField, WireKind, wire_hash};
use engine_webrtc_protocol::packed::{self, PackedOp, PackedSchema, WireValue};

const VECTOR: &str = include_str!("packed_golden.json");

fn vector() -> serde_json::Value {
    serde_json::from_str(VECTOR).expect("the golden file is valid JSON")
}

fn kind_from_code(code: u64) -> WireKind {
    WireKind::from_code(code as u8).expect("the vector uses a known wire kind")
}

fn schema_of(vector: &serde_json::Value) -> SchemaDef {
    let schema = &vector["schema"];
    let fields = schema["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .map(|field| {
            let kind = kind_from_code(field["kind"].as_u64().expect("kind"));
            let bits = field["bits"].as_u64().expect("bits") as u8;
            let min = field["min"].as_f64().expect("min") as f32;
            let max = field["max"].as_f64().expect("max") as f32;
            let integer = field["integer"].as_bool().expect("integer");
            let mut encoding = FieldEncoding::range(bits, min, max);
            if integer {
                encoding = encoding.as_integer();
            }
            SchemaField {
                name: field["name"].as_str().expect("name").into(),
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
        })
        .collect();
    SchemaDef {
        name: schema["name"].as_str().expect("name").into(),
        version: schema["version"].as_u64().expect("version") as u32,
        fields,
        line: 0,
        column: 0,
    }
}

fn wire_value(value: &serde_json::Value, kind: WireKind) -> WireValue {
    match kind {
        WireKind::U8 => WireValue::U8(value.as_u64().unwrap() as u8),
        WireKind::U16 => WireValue::U16(value.as_u64().unwrap() as u16),
        WireKind::U32 => WireValue::U32(value.as_u64().unwrap() as u32),
        WireKind::I8 => WireValue::I8(value.as_i64().unwrap() as i8),
        WireKind::I16 => WireValue::I16(value.as_i64().unwrap() as i16),
        WireKind::I32 => WireValue::I32(value.as_i64().unwrap() as i32),
        WireKind::I64 => WireValue::I64(value.as_i64().unwrap()),
        WireKind::F32 => WireValue::F32(value.as_f64().unwrap() as f32),
        WireKind::F64 => WireValue::F64(value.as_f64().unwrap()),
        WireKind::Bool => WireValue::Bool(value.as_bool().unwrap()),
        WireKind::String => WireValue::Str(value.as_str().unwrap_or_default().into()),
        WireKind::Array => WireValue::Arr(vec![]),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn the_schema_fingerprint_in_the_file_matches_the_compiler() {
    let vector = vector();
    let schema = schema_of(&vector);
    let expected = vector["schema"]["fingerprint"]
        .as_str()
        .expect("fingerprint");
    assert_eq!(
        format!("{:#010x}", wire_hash(&schema)),
        expected,
        "the file's schema no longer hashes the same — a field or an encoding changed"
    );
}

#[test]
fn rust_encodes_exactly_the_bytes_in_the_file() {
    let vector = vector();
    let schema = schema_of(&vector);
    let packed = PackedSchema::from_def(&schema);

    let row = &vector["full_row"];
    let values: Vec<WireValue> = row["values"]
        .as_array()
        .expect("values")
        .iter()
        .zip(schema.fields.iter())
        .map(|(value, field)| wire_value(value, field.kind))
        .collect();
    let id = row["id"].as_u64().expect("id") as u32;

    let mut clamped = 0;
    let bytes = packed::encode_full(&packed, &[(id, values)], &mut clamped).expect("encode");
    assert_eq!(clamped, 0);
    assert_eq!(
        hex(&bytes),
        vector["full_hex"].as_str().expect("full_hex"),
        "the full row's bytes changed"
    );

    let mut ops = Vec::new();
    for op in vector["patch"].as_array().expect("patch") {
        match op["op"].as_str().expect("op") {
            "remove" => ops.push(PackedOp::Remove {
                id: op["id"].as_u64().expect("id") as u32,
            }),
            "update" => {
                let values = op["values"]
                    .as_array()
                    .expect("values")
                    .iter()
                    .map(|pair| {
                        let index = pair[0].as_u64().expect("index") as usize;
                        (index, wire_value(&pair[1], schema.fields[index].kind))
                    })
                    .collect();
                ops.push(PackedOp::Update {
                    id: op["id"].as_u64().expect("id") as u32,
                    mask: op["mask"].as_u64().expect("mask") as u32,
                    values,
                });
            }
            other => panic!("unknown op {other}"),
        }
    }
    let bytes = packed::encode_patch(&packed, &ops, &mut clamped).expect("encode");
    assert_eq!(
        hex(&bytes),
        vector["patch_hex"].as_str().expect("patch_hex"),
        "the patch's bytes changed"
    );
}

#[test]
fn the_file_still_decodes_to_the_values_it_names() {
    let vector = vector();
    let schema = schema_of(&vector);
    let packed = PackedSchema::from_def(&schema);
    let bytes = hex_bytes(vector["full_hex"].as_str().expect("full_hex"));

    let decoded = packed::decode(&packed, &bytes).expect("the golden bytes decode");
    match &decoded.ops[0] {
        PackedOp::Add { id, values } => {
            assert_eq!(*id, 7);
            assert_eq!(values[0], WireValue::U8(100));
            assert_eq!(values[2], WireValue::Bool(true));
            assert_eq!(values[3], WireValue::Str("ab".into()));
            assert_eq!(values[4], WireValue::U32(42));
        }
        other => panic!("expected an add, got {other:?}"),
    }
}

fn hex_bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex"))
        .collect()
}
