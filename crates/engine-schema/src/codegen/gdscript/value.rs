use crate::{SchemaField, WireKind};

pub(super) fn gd_scalar(kind: WireKind) -> &'static str {
    match kind {
        WireKind::F32 | WireKind::F64 => "float",
        WireKind::Bool => "bool",
        WireKind::String => "String",
        WireKind::Array => "Array",
        _ => "int",
    }
}

pub(super) fn gd_type(field: &SchemaField) -> &'static str {
    gd_scalar(field.kind)
}

pub(super) fn gd_default(field: &SchemaField) -> &'static str {
    if field.optional() {
        return "null";
    }
    match field.kind {
        WireKind::String => "\"\"",
        WireKind::Array => "[]",
        WireKind::Bool => "false",
        WireKind::F32 | WireKind::F64 => "0.0",
        _ => "0",
    }
}

pub(super) fn gd_convert(kind: WireKind) -> &'static str {
    match kind {
        WireKind::F32 | WireKind::F64 => "float",
        WireKind::Bool => "bool",
        WireKind::String => "str",
        WireKind::Array => "",
        _ => "int",
    }
}
