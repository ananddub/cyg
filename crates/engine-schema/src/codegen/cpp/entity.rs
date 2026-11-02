use std::fmt::Write as _;

use super::cases::{cpp_get_case, cpp_set_case};
use super::ident::cpp_ident;
use super::table::cpp_field_table;
use super::value::{cpp_default, cpp_type};
use crate::SchemaField;

pub(super) fn emit_cpp_entity(
    out: &mut String,
    name: &str,
    fields: &[SchemaField],
    version: u32,
    hash: u32,
    doc: &str,
) {
    let _ = writeln!(out, "/// {doc}");
    let _ = writeln!(out, "struct {name} final : public Entity {{");
    let _ = writeln!(
        out,
        "    inline static constexpr uint32_t kVersion = {version}u;"
    );
    let _ = writeln!(out, "    inline static constexpr uint32_t kHash = {hash}u;");
    out.push_str("    inline static const std::vector<FieldDef> kFields = {\n");
    out.push_str(&cpp_field_table(fields));
    out.push_str("    };\n\n");
    for field in fields {
        let _ = writeln!(
            out,
            "    {} {} = {};",
            cpp_type(field),
            cpp_ident(&field.name),
            cpp_default(field)
        );
    }
    if !fields.is_empty() {
        out.push('\n');
    }
    out.push_str(
        "    int field_count() const override { return static_cast<int>(kFields.size()); }\n\n",
    );
    out.push_str("    void set_field(int field, const Value& value) override {\n");
    out.push_str("        switch (field) {\n");
    for (index, field) in fields.iter().enumerate() {
        let case = cpp_set_case(field, &cpp_ident(&field.name));
        out.push_str(&case.replace("{index}", &index.to_string()));
    }
    out.push_str("            default: break;\n        }\n    }\n\n");
    out.push_str("    Value get_field(int field) const override {\n");
    out.push_str("        switch (field) {\n");
    for (index, field) in fields.iter().enumerate() {
        out.push_str(&cpp_get_case(field, &cpp_ident(&field.name), index));
    }
    out.push_str("            default: return Value::null();\n        }\n    }\n\n");
    out.push_str("    void clear_fields() override {\n");
    for field in fields {
        let _ = writeln!(
            out,
            "        {} = {};",
            cpp_ident(&field.name),
            cpp_default(field)
        );
    }
    out.push_str("    }\n};\n\n");
}
