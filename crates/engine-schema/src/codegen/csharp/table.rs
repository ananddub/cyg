use std::fmt::Write as _;

use super::value::{cs_base_type, cs_type};
use crate::codegen::common::{escape_string, kind_size};
use crate::{SchemaField, WireKind};

pub(super) fn cs_field_table(fields: &[SchemaField]) -> String {
    let mut out = String::new();
    for field in fields {
        let _ = writeln!(
            out,
            "            new OxFieldDef(\"{}\", OxWireKind::{:?}, {}, OxWireKind::{:?}, {}),",
            escape_string(&field.name),
            field.kind,
            field.flags,
            field.element.unwrap_or(WireKind::U8),
            kind_size(field)
        );

    }
    out.replace("OxWireKind::", "OxWireKind.")
}

pub(super) fn cs_field_read(field: &SchemaField, index: usize) -> String {
    let _ = index;
    if !field.optional() {
        return match field.kind {
            WireKind::String | WireKind::Array => {
                format!("({})value;", cs_base_type(field))
            }
            _ => format!("({})value;", cs_base_type(field)),
        };
    }
    match field.kind {
        WireKind::String | WireKind::Array => format!("({})value;", cs_base_type(field)),
        _ => format!(
            "value == null ? ({ty})null : ({base})value;",
            ty = cs_type(field),
            base = cs_base_type(field)
        ),
    }
}
