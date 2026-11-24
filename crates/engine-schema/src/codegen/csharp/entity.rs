use std::fmt::Write as _;

use super::ident::cs_ident;
use super::table::{cs_field_read, cs_field_table};
use super::value::{cs_default, cs_type};
use crate::SchemaField;

pub(super) fn emit_cs_entity(
    out: &mut String,
    name: &str,
    fields: &[SchemaField],
    version: u32,
    hash: u32,
    doc: &str,
) {
    let _ = writeln!(out, "    /// <summary>{doc}</summary>");
    let _ = writeln!(out, "    public sealed class {name} : CygSchemaEntity");
    out.push_str("    {\n");
    let _ = writeln!(out, "        public const uint SchemaVersion = {version}u;");
    let _ = writeln!(out, "        public const uint SchemaHash = {hash}u;");
    let _ = writeln!(
        out,
        "        public const int FieldCount = {};",
        fields.len()
    );
    let _ = writeln!(
        out,
        "        public static readonly CygFieldDef[] Fields = new CygFieldDef[FieldCount]"
    );
    out.push_str("        {\n");
    out.push_str(&cs_field_table(fields));
    out.push_str("        };\n\n");

    for field in fields {
        let _ = writeln!(
            out,
            "        public {} {} = {};",
            cs_type(field),
            cs_ident(&field.name),
            cs_default(field)
        );
    }
    if !fields.is_empty() {
        out.push('\n');
    }

    out.push_str("        public override int CygFieldCount { get { return FieldCount; } }\n\n");
    out.push_str("        public override void CygSet(int field, object value)\n        {\n");
    out.push_str("            switch (field)\n            {\n");
    for (index, field) in fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "                case {index}: {} = {} break;",
            cs_ident(&field.name),
            cs_field_read(field, index)
        );
    }
    out.push_str("            }\n        }\n\n");

    out.push_str("        public override object CygGet(int field)\n        {\n");
    out.push_str("            switch (field)\n            {\n");
    for (index, field) in fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "                case {index}: return {};",
            cs_ident(&field.name)
        );
    }
    out.push_str("            }\n            return null;\n        }\n\n");

    out.push_str("        public override void CygClear()\n        {\n");
    for field in fields {
        let _ = writeln!(
            out,
            "            {} = {};",
            cs_ident(&field.name),
            cs_default(field)
        );
    }
    out.push_str("        }\n    }\n\n");
}
