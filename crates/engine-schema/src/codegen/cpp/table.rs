use std::fmt::Write as _;

use crate::codegen::common::{escape_string, kind_size};
use crate::{SchemaField, WireKind};

pub(super) fn cpp_field_table(fields: &[SchemaField]) -> String {
    let mut out = String::new();
    for field in fields {
        let _ = writeln!(
            out,
            "        {{\"{}\", WireKind::{:?}, {}, WireKind::{:?}, {}}},",
            escape_string(&field.name),
            field.kind,
            field.flags,
            field.element.unwrap_or(WireKind::U8),
            kind_size(field)
        );
    }
    out
}

pub(super) fn cpp_value_make(kind: WireKind, expr: &str) -> String {
    match kind {
        WireKind::U8 => format!("Value::make_u8({expr})"),
        WireKind::I8 => format!("Value::make_i8({expr})"),
        WireKind::U16 => format!("Value::make_u16({expr})"),
        WireKind::I16 => format!("Value::make_i16({expr})"),
        WireKind::U32 => format!("Value::make_u32({expr})"),
        WireKind::I32 => format!("Value::make_i32({expr})"),
        WireKind::F32 => format!("Value::make_f32({expr})"),
        WireKind::F64 => format!("Value::make_f64({expr})"),
        WireKind::Bool => format!("Value::make_bool({expr})"),
        WireKind::I64 => format!("Value::make_i64({expr})"),
        WireKind::String => format!("Value::make_string({expr})"),
        WireKind::Array => format!("Value::make_array({expr})"),
    }
}

pub(super) fn cpp_value_read(kind: WireKind, expr: &str) -> String {
    match kind {
        WireKind::U8 => format!("{expr}.as_u8()"),
        WireKind::I8 => format!("{expr}.as_i8()"),
        WireKind::U16 => format!("{expr}.as_u16()"),
        WireKind::I16 => format!("{expr}.as_i16()"),
        WireKind::U32 => format!("{expr}.as_u32()"),
        WireKind::I32 => format!("{expr}.as_i32()"),
        WireKind::F32 => format!("{expr}.as_f32()"),
        WireKind::F64 => format!("{expr}.as_f64()"),
        WireKind::Bool => format!("{expr}.as_bool()"),
        WireKind::I64 => format!("{expr}.as_i64()"),
        WireKind::String => format!("{expr}.as_string()"),
        WireKind::Array => format!("{expr}.as_array()"),
    }
}
