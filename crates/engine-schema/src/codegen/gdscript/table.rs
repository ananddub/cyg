use std::fmt::Write as _;

use super::ident::gd_ident;
use super::value::{gd_convert, gd_default, gd_type};
use crate::SchemaField;
use crate::codegen::common::{escape_string, kind_name, kind_size};

pub(super) fn gd_field_meta(fields: &[SchemaField], indent: &str) -> String {
    let mut out = String::new();
    for field in fields {
        let element = field.element.map(kind_name).unwrap_or("none");
        let _ = writeln!(
            out,
            "{indent}{{\"name\": \"{}\", \"kind\": \"{}\", \"code\": {}, \"flags\": {}, \"element\": \"{}\", \"size\": {}}},",
            escape_string(&field.name),
            kind_name(field.kind),
            field.kind.code(),
            field.flags,
            element,
            kind_size(field)
        );
    }
    out
}

pub(super) fn emit_gd_entity(
    out: &mut String,
    name: &str,
    fields: &[SchemaField],
    version: u32,
    hash: u32,
) {
    let _ = writeln!(out, "class {name} extends CYGSchema.Entity:");
    let _ = writeln!(out, "    const SCHEMA_VERSION := {version}");
    let _ = writeln!(out, "    const SCHEMA_HASH := {hash}");
    if fields.is_empty() {
        out.push_str("    pass\n\n");
        return;
    }
    for field in fields {
        if field.optional() {
            let _ = writeln!(out, "    var {} = null", gd_ident(&field.name));
        } else {
            let _ = writeln!(
                out,
                "    var {}: {} = {}",
                gd_ident(&field.name),
                gd_type(field),
                gd_default(field)
            );
        }
    }
    out.push('\n');

    out.push_str("    func set_field(field: int, value) -> void:\n");
    out.push_str("        match field:\n");
    for (index, field) in fields.iter().enumerate() {
        let ident = gd_ident(&field.name);
        let convert = gd_convert(field.kind);
        let _ = writeln!(out, "            {index}:");
        if field.optional() {
            let _ = writeln!(
                out,
                "                {ident} = null if value == null else {}(value)",
                if convert.is_empty() { "value" } else { convert }
            );
        } else if convert.is_empty() {
            let _ = writeln!(out, "                {ident} = value");
        } else {
            let _ = writeln!(out, "                {ident} = {convert}(value)");
        }
    }
    out.push_str("            _:\n                pass\n\n");

    out.push_str("    func get_field(field: int):\n");
    out.push_str("        match field:\n");
    for (index, field) in fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "            {index}:\n                return {}",
            gd_ident(&field.name)
        );
    }
    out.push_str("        return null\n\n");

    out.push_str("    func clear_fields() -> void:\n");
    for field in fields {
        let _ = writeln!(
            out,
            "        {} = {}",
            gd_ident(&field.name),
            gd_default(field)
        );
    }
    out.push('\n');
}
