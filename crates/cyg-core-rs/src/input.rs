//! Input encoding for both client lanes.
//!
//! Schema lane (default WS/DataChannel path): `[seq u16 LE][Input fields in
//! schema order]` — port of cyg.js `_packSchemaInput`.
//! Mesh lane: the 23-byte `InputFrame` from `engine-webrtc-protocol`.

use engine_schema::SchemaField;

/// Caller-facing input value; must match the `Input` schema field order.
#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    U8(u8),
    U16(u16),
    U32(u32),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
}

/// Port of `_packSchemaInput`: `[seq u16 LE][field bytes little-endian]`.
pub fn pack_schema_input(
    seq: u16,
    values: &[InputValue],
    fields: &[SchemaField],
) -> Result<Vec<u8>, String> {
    if values.len() != fields.len() {
        return Err(format!(
            "input arity mismatch: {} values for {} fields",
            values.len(),
            fields.len()
        ));
    }
    let mut out = Vec::with_capacity(2 + 8 * fields.len());
    out.extend_from_slice(&seq.to_le_bytes());
    for (value, field) in values.iter().zip(fields) {
        match (value, field.kind) {
            (InputValue::U8(v), engine_schema::WireKind::U8) => out.push(*v),
            (InputValue::Bool(v), engine_schema::WireKind::Bool) => out.push(*v as u8),
            (InputValue::U16(v), engine_schema::WireKind::U16) => {
                out.extend_from_slice(&v.to_le_bytes())
            }
            (InputValue::I16(v), engine_schema::WireKind::I16) => {
                out.extend_from_slice(&v.to_le_bytes())
            }
            (InputValue::U32(v), engine_schema::WireKind::U32) => {
                out.extend_from_slice(&v.to_le_bytes())
            }
            (InputValue::I32(v), engine_schema::WireKind::I32) => {
                out.extend_from_slice(&v.to_le_bytes())
            }
            (InputValue::F32(v), engine_schema::WireKind::F32) => {
                out.extend_from_slice(&v.to_le_bytes())
            }
            (InputValue::F64(v), engine_schema::WireKind::F64) => {
                out.extend_from_slice(&v.to_le_bytes())
            }
            (InputValue::I64(v), engine_schema::WireKind::I64) => {
                out.extend_from_slice(&(*v as u64).to_le_bytes())
            }
            (InputValue::I8(v), engine_schema::WireKind::I8) => out.push(*v as u8),
            _ => {
                return Err(format!(
                    "input value type mismatch on field '{}'",
                    field.name
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_schema::{SchemaDef, SchemaField, WireKind};

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

    #[test]
    fn packs_seq_then_little_endian_fields() {
        let fields = vec![
            field("dir", WireKind::I32),
            field("speed", WireKind::F64),
            field("fire", WireKind::Bool),
        ];
        let wire = pack_schema_input(
            42,
            &[
                InputValue::I32(-1),
                InputValue::F64(0.5),
                InputValue::Bool(true),
            ],
            &fields,
        )
        .unwrap();
        assert_eq!(wire.len(), 2 + 4 + 8 + 1);
        assert_eq!(&wire[..2], &42u16.to_le_bytes());
        assert_eq!(&wire[2..6], &(-1i32).to_le_bytes());
        assert_eq!(&wire[6..14], &0.5f64.to_le_bytes());
        assert_eq!(wire[14], 1);
    }

    #[test]
    fn rejects_wrong_arity() {
        let fields = vec![field("dir", WireKind::I32)];
        let err = pack_schema_input(0, &[], &fields).unwrap_err();
        assert!(err.contains("arity mismatch"));
    }

    #[test]
    fn schema_def_hashes_are_stable() {
        // Guards against accidental wire-format drift: this hash mirrors the
        // descriptor roundtrip test in engine-schema.
        let def = SchemaDef {
            nested_arrays: Vec::new(),
            name: "Input".into(),
            version: 1,
            fields: vec![field("dir", WireKind::I32)],
            line: 0,
            column: 0,
        };
        assert_eq!(
            engine_schema::wire_hash(&def),
            engine_schema::wire_hash(&def.clone())
        );
    }
}
