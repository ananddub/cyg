use std::fmt::Write as _;

use super::table::{cpp_value_make, cpp_value_read};
use super::value::{cpp_base_type, cpp_scalar};
use crate::{SchemaField, WireKind};

pub(super) fn cpp_set_case(field: &SchemaField, ident: &str) -> String {
    let mut out = String::new();
    if field.kind == WireKind::Array {
        let element = field.element.unwrap_or(WireKind::F64);
        let read = cpp_value_read(element, "item");
        out.push_str("            case {index}: {\n");
        if field.optional() {
            out.push_str("                if (value.is_null()) { ");
            let _ = writeln!(out, "{ident}.reset(); break; }}");
            let _ = writeln!(
                out,
                "                std::vector<{}> next;",
                cpp_scalar(element)
            );
        } else {
            let _ = writeln!(out, "                {ident}.clear();");
            let _ = writeln!(
                out,
                "                std::vector<{}> next;",
                cpp_scalar(element)
            );
        }
        out.push_str("                const auto& items = value.as_array();\n");
        out.push_str("                next.reserve(items.size());\n");
        let _ = writeln!(
            out,
            "                for (const auto& item : items) next.push_back({read});"
        );
        if field.optional() {
            let _ = writeln!(out, "                {ident} = std::move(next);");
        } else {
            let _ = writeln!(out, "                {ident} = std::move(next);");
        }
        out.push_str("                break;\n            }\n");
    } else if field.optional() {
        let read = cpp_value_read(field.kind, "value");
        let _ = writeln!(
            out,
            "            case {{index}}: {ident} = value.is_null() ? std::nullopt : std::optional<{}>({}); break;",
            cpp_base_type(field),
            read
        );
    } else {
        let read = cpp_value_read(field.kind, "value");
        let _ = writeln!(out, "            case {{index}}: {ident} = {read}; break;");
    }
    out
}

pub(super) fn cpp_get_case(field: &SchemaField, ident: &str, index: usize) -> String {
    let mut out = String::new();
    if field.kind == WireKind::Array {
        let element = field.element.unwrap_or(WireKind::F64);
        let _ = writeln!(out, "            case {index}: {{");
        if field.optional() {
            let _ = writeln!(out, "                if (!{ident}) return Value::null();");
            out.push_str("                std::vector<Value> items;\n");
            let _ = writeln!(out, "                items.reserve({ident}->size());");
            let _ = writeln!(
                out,
                "                for (const auto& item : *{ident}) items.push_back({});",
                cpp_value_make(element, "item")
            );
            out.push_str(
                "                return Value::make_array(std::move(items));\n            }\n",
            );
        } else {
            out.push_str("                std::vector<Value> items;\n");
            let _ = writeln!(out, "                items.reserve({ident}.size());");
            let _ = writeln!(
                out,
                "                for (const auto& item : {ident}) items.push_back({});",
                cpp_value_make(element, "item")
            );
            out.push_str(
                "                return Value::make_array(std::move(items));\n            }\n",
            );
        }
    } else if field.optional() {
        let _ = writeln!(out, "            case {index}:");
        let _ = writeln!(
            out,
            "                return {ident} ? {} : Value::null();",
            cpp_value_make(field.kind, format!("*{ident}").as_str())
        );
    } else {
        let _ = writeln!(
            out,
            "            case {index}: return {};",
            cpp_value_make(field.kind, ident)
        );
    }
    out
}
