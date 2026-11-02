use crate::{SchemaField, WireKind};

pub(super) fn cpp_scalar(kind: WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "uint8_t",
        WireKind::I8 => "int8_t",
        WireKind::U16 => "uint16_t",
        WireKind::I16 => "int16_t",
        WireKind::U32 => "uint32_t",
        WireKind::I32 => "int32_t",
        WireKind::F32 => "float",
        WireKind::F64 => "double",
        WireKind::Bool => "bool",
        WireKind::I64 => "int64_t",
        WireKind::String => "std::string",
        WireKind::Array => "std::vector<Value>",
    }
}

pub(super) fn cpp_base_type(field: &SchemaField) -> String {
    if field.kind == WireKind::Array {
        format!(
            "std::vector<{}>",
            cpp_scalar(field.element.unwrap_or(WireKind::F64))
        )
    } else {
        cpp_scalar(field.kind).to_string()
    }
}

pub(super) fn cpp_type(field: &SchemaField) -> String {
    let base = cpp_base_type(field);
    if field.optional() {
        format!("std::optional<{base}>")
    } else {
        base
    }
}

pub(super) fn cpp_default(field: &SchemaField) -> &'static str {
    if field.optional() {
        return "std::nullopt";
    }
    match field.kind {
        WireKind::Bool => "false",
        WireKind::F32 => "0.0f",
        WireKind::F64 => "0.0",
        _ => "{}",
    }
}
