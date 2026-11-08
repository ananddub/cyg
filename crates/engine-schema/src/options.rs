//! Join-options encoder — the host-side mirror of the generated
//! `decodeJoinOptions(r: InputReader)`. The client sends its join options as
//! JSON; when the schema declares a `JoinOptions` def the host re-encodes
//! them into that field order, so the room's `onJoin(id, options)` is typed
//! and no JSON parsing ever happens in the guest.

use crate::model::SchemaField;
use crate::wire::WireKind;

/// Encode join-options JSON into the `JoinOptions` field order. Missing or
/// wrong-typed keys fall back to the schema's own `.default(...)`, then to
/// zero (`0`, `false`, `""`) — lenient like Colyseus. Garbage JSON encodes as
/// defaults rather than failing the join.
pub fn encode_join_options(fields: &[SchemaField], json: &str) -> Vec<u8> {
    let value = serde_json::from_str::<serde_json::Value>(json).unwrap_or(serde_json::Value::Null);
    let object = value.as_object();
    let mut out = Vec::new();
    for field in fields {
        let value = object.and_then(|object| object.get(&field.name));
        encode_field(&mut out, field, value);
    }
    out
}

fn encode_field(out: &mut Vec<u8>, field: &SchemaField, value: Option<&serde_json::Value>) {
    match field.kind {
        WireKind::Bool => out.push(
            value
                .and_then(|value| value.as_bool())
                .or_else(|| field.default.as_deref().map(|text| text == "true"))
                .unwrap_or(false) as u8,
        ),
        WireKind::U8 => out.push(num(value, field).clamp(0.0, 255.0) as u8),
        WireKind::I8 => out.push(num(value, field).clamp(-128.0, 127.0) as i8 as u8),
        WireKind::U16 => {
            out.extend_from_slice(&(num(value, field).clamp(0.0, 65535.0) as u16).to_le_bytes())
        }
        WireKind::I16 => out
            .extend_from_slice(&(num(value, field).clamp(-32768.0, 32767.0) as i16).to_le_bytes()),
        WireKind::U32 => out.extend_from_slice(
            &(num(value, field).clamp(0.0, u32::MAX as f64) as u32).to_le_bytes(),
        ),
        WireKind::I32 => out.extend_from_slice(
            &(num(value, field).clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_le_bytes(),
        ),
        WireKind::I64 => out.extend_from_slice(&(num(value, field) as i64).to_le_bytes()),
        WireKind::F32 => out.extend_from_slice(&(num(value, field) as f32).to_le_bytes()),
        WireKind::F64 => out.extend_from_slice(&num(value, field).to_le_bytes()),
        WireKind::String => {
            let text = value
                .and_then(|value| value.as_str())
                .map(|text| text.to_string())
                .or_else(|| field.default.clone())
                .unwrap_or_default();
            let mut bytes = text.as_bytes();
            if bytes.len() > u16::MAX as usize {
                bytes = &bytes[..u16::MAX as usize];
            }
            out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        // Arrays are state-only; the parse layer never lets one reach here.
        WireKind::Array => encode_field(
            out,
            &SchemaField {
                kind: WireKind::F64,
                ..field.clone()
            },
            value,
        ),
    }
}

/// JSON number as f64; the field's declared `.default(...)` when the value is
/// absent or not numeric; `0` when there is no default either.
fn num(value: Option<&serde_json::Value>, field: &SchemaField) -> f64 {
    value
        .and_then(|value| value.as_f64())
        .or_else(|| field.default.as_deref().and_then(|text| text.parse().ok()))
        .unwrap_or(0.0)
}
