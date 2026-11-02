use crate::{RootState, SchemaDef, SchemaField, WireKind};

pub(crate) fn sanitize_source(source_name: &str) -> String {
    source_name.replace(['\r', '\n'], " ")
}

pub(crate) fn escape_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

pub(crate) fn root_hash(root: &RootState) -> u32 {
    crate::wire_hash(&SchemaDef {
        name: root.name.clone(),
        version: root.version,
        fields: root.scalars.clone(),
        line: 0,
        column: 0,
    })
}

pub(crate) fn root_class_name(root: &RootState, schemas: &[SchemaDef]) -> String {
    if schemas.iter().any(|schema| schema.name == root.name) {
        format!("{}Root", root.name)
    } else {
        root.name.clone()
    }
}

pub(crate) fn kind_name(kind: WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "u8",
        WireKind::I8 => "i8",
        WireKind::U16 => "u16",
        WireKind::I16 => "i16",
        WireKind::U32 => "u32",
        WireKind::I32 => "i32",
        WireKind::F32 => "f32",
        WireKind::F64 => "f64",
        WireKind::Bool => "bool",
        WireKind::I64 => "i64",
        WireKind::String => "string",
        WireKind::Array => "array",
    }
}

pub(crate) fn kind_size(field: &SchemaField) -> usize {
    field.kind.size()
}
