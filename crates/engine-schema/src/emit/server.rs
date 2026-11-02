use super::common::{default_literal, emit_accessors, emit_field_builder, HEADER};
use super::world::emit_world;
use crate::model::{RootState, SchemaDef};

pub(crate) fn emit_server(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str(&format!(
        "// Generated from {source_name} by `ox-game schema`. Edit the schema source, not this file.\n\n"
    ));
    if is_multi_root(root) {
        out.push_str(
            "import { SchemaDef, SchemaEntity, SchemaState, SchemaWorld } from \"./cyg-server\";\n",
        );
    } else {
        out.push_str("import { SchemaDef, SchemaEntity } from \"./cyg-server\";\n");
    }

    for schema in schemas {
        out.push('\n');
        out.push_str(&format!(
            "export const {}Schema = new SchemaDef(\"{}\", {})\n",
            schema.name, schema.name, schema.version
        ));
        for field in &schema.fields {
            emit_field_builder(&mut out, field);
        }

        out.pop();
        out.push_str(";\n\n");

        for field in &schema.fields {
            out.push_str(&format!(
                "const _{schema}_{field}: i32 = {schema}Schema.indexOf(\"{field}\");\n",
                schema = schema.name,
                field = field.name
            ));
        }
        out.push('\n');

        out.push_str(&format!(
            "export class {} extends SchemaEntity {{\n",
            schema.name
        ));
        out.push_str("    constructor() {\n        super(");
        out.push_str(&schema.name);
        out.push_str("Schema);\n");
        for field in &schema.fields {
            if let Some(default) = &field.default {
                out.push_str(&format!(
                    "        this.set{}(_{schema}_{name}, {literal});\n",
                    field.kind.value_suffix(),
                    schema = schema.name,
                    name = field.name,
                    literal = default_literal(field.kind, default),
                ));
            }
        }
        out.push_str("    }\n");
        for field in &schema.fields {
            emit_accessors(&mut out, &format!("_{}", schema.name), field);
        }
        out.push_str("}\n");
    }

    if let Some(root) = root.filter(|root| is_multi_root(Some(root))) {
        emit_world(&mut out, root);
    }
    out
}

fn is_multi_root(root: Option<&RootState>) -> bool {
    root.map(|root| !root.scalars.is_empty() || root.collections.len() != 1)
        .unwrap_or(false)
}
