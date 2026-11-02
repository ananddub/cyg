use crate::WireKind;

pub(super) fn primitive_kind(name: &str) -> Option<WireKind> {
    match name {
        "number" | "float64" | "f64" => Some(WireKind::F64),
        "float32" | "f32" => Some(WireKind::F32),
        "int8" | "i8" => Some(WireKind::I8),
        "uint8" | "u8" => Some(WireKind::U8),
        "int16" | "i16" => Some(WireKind::I16),
        "uint16" | "u16" => Some(WireKind::U16),
        "int32" | "i32" => Some(WireKind::I32),
        "uint32" | "u32" => Some(WireKind::U32),
        "int64" | "i64" => Some(WireKind::I64),
        "boolean" | "bool" => Some(WireKind::Bool),
        "string" => Some(WireKind::String),
        _ => None,
    }
}

pub(super) fn is_map_member_name(name: &str) -> bool {
    matches!(
        name,
        "size"
            | "get"
            | "set"
            | "has"
            | "delete"
            | "clear"
            | "keys"
            | "values"
            | "entries"
            | "forEach"
            | "toJSON"
            | "onAdd"
            | "onRemove"
            | "onChange"
    )
}
