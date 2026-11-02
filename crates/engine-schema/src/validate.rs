use std::collections::HashMap;

use crate::model::{SchemaDef, SchemaDiagnostic};

pub(crate) const MAX_FIELDS: usize = 32;

pub(crate) fn check_schema_name(
    name: &str,
    line: usize,
    column: usize,
    diagnostics: &mut Vec<SchemaDiagnostic>,
) {
    if is_reserved_word(name) {
        diagnostics.push(SchemaDiagnostic {
            line,
            column,
            message: format!("schema name '{name}' is a reserved word"),
        });
    }
}

pub(crate) fn check_unique_names(schemas: &[SchemaDef], diagnostics: &mut Vec<SchemaDiagnostic>) {
    let mut seen: HashMap<&str, (usize, usize)> = HashMap::new();
    for schema in schemas {
        if let Some((line, column)) = seen.insert(&schema.name, (schema.line, schema.column)) {
            diagnostics.push(SchemaDiagnostic {
                line: schema.line,
                column: schema.column,
                message: format!(
                    "schema '{name}' is declared more than once (first at {line}:{column})",
                    name = schema.name
                ),
            });
        }
    }
}

pub(crate) fn is_reserved_field_name(name: &str) -> bool {
    matches!(
        name,
        "id" | "schemaHash" | "dirtyMask" | "clearDirty" | "schema"
    ) || is_reserved_word(name)
}

fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "constructor"
            | "continue"
            | "debugger"
            | "declare"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "from"
            | "function"
            | "get"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "namespace"
            | "new"
            | "null"
            | "of"
            | "package"
            | "private"
            | "protected"
            | "prototype"
            | "public"
            | "return"
            | "set"
            | "static"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "bool"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "string"
            | "SchemaDef"
            | "SchemaEntity"
            | "SchemaState"
            | "CYG"
    )
}
