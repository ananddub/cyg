use crate::{SchemaField, WireKind};

pub(super) fn cs_scalar(kind: WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "byte",
        WireKind::I8 => "sbyte",
        WireKind::U16 => "ushort",
        WireKind::I16 => "short",
        WireKind::U32 => "uint",
        WireKind::I32 => "int",
        WireKind::F32 => "float",
        WireKind::F64 => "double",
        WireKind::Bool => "bool",
        WireKind::I64 => "long",
        WireKind::String => "string",
        WireKind::Array => "double[]",
    }
}

pub(super) fn cs_base_type(field: &SchemaField) -> String {
    if field.kind == WireKind::Array {
        format!("{}[]", cs_scalar(field.element.unwrap_or(WireKind::F64)))
    } else {
        cs_scalar(field.kind).to_string()
    }
}

pub(super) fn cs_type(field: &SchemaField) -> String {
    let base = cs_base_type(field);
    if field.optional() && !matches!(field.kind, WireKind::String | WireKind::Array) {
        format!("{base}?")
    } else {
        base
    }
}

pub(super) fn cs_default(field: &SchemaField) -> String {
    if field.optional() {
        return "null".into();
    }
    match field.kind {
        WireKind::String => "string.Empty".into(),
        WireKind::Array => format!(
            "new {}[0]",
            cs_scalar(field.element.unwrap_or(WireKind::F64))
        ),
        WireKind::Bool => "false".into(),
        WireKind::F32 => "0.0f".into(),
        WireKind::F64 => "0.0".into(),
        _ => "0".into(),
    }
}
