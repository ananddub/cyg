use super::common::{field_typescript_type, HEADER};
use crate::model::{RootState, SchemaDef};

pub(crate) fn emit_client_dts(
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
    out.push_str(
        "import type { Room as CYGRoom, SchemaDef, SchemaEntity, SchemaMap } from \"./cyg-client\";\n",
    );

    for schema in schemas {
        out.push('\n');
        out.push_str(&format!(
            "export declare const {}Schema: SchemaDef;\n",
            schema.name
        ));
        out.push_str(&format!(
            "/** One decoded row from `{}Schema` (wire version {}). */\n",
            schema.name, schema.version
        ));
        out.push_str(&format!("export interface {}State {{\n", schema.name));
        out.push_str("    id: number;\n");
        for field in &schema.fields {
            out.push_str(&format!(
                "    {name}: {ty};\n",
                name = field.name,
                ty = field_typescript_type(field)
            ));
        }
        out.push_str("}\n");
    }

    if let Some(root) = root {
        out.push('\n');
        out.push_str("/** Typed root state exposed as `room.state`. */\n");
        out.push_str("export interface GameState {\n");
        for field in &root.scalars {
            out.push_str(&format!(
                "    {name}: {ty};\n",
                name = field.name,
                ty = field_typescript_type(field)
            ));
        }
        for collection in &root.collections {
            let row = schemas
                .iter()
                .find(|schema| schema.name == collection.schema)
                .map(|schema| format!("{}State", schema.name))
                .unwrap_or_else(|| "SchemaEntity".to_string());
            out.push_str(&format!(
                "    {name}: SchemaMap<{row}>;\n",
                name = collection.name,
                row = row
            ));
        }
        out.push_str("}\n");
        out.push_str("\n/** Convenience alias for a room carrying this schema. */\n");
        out.push_str("export type GameRoom = CYGRoom<GameState>;\n");
    }

    out.push_str("\nexport declare const schemas: {\n");
    for schema in schemas {
        out.push_str(&format!("    {0}: SchemaDef;\n", schema.name));
    }
    out.push_str("};\n");

    out.push_str("\ndeclare global {\n    interface Window {\n        CYGSchema: {\n");
    for schema in schemas {
        out.push_str(&format!("            {0}Schema: SchemaDef;\n", schema.name));
    }
    out.push_str("            schemas: typeof schemas;\n        };\n    }\n}\n");

    out.push_str(
        "\n/** Type shim for `schema.ts` when cyg is not installed. */\n\
         declare module \"cyg\" {\n\
         \x20   export interface SchemaType<T> {\n\
         \x20       extend<B extends Record<string, unknown>>(base: B): SchemaType<T & B> & (T & B);\n\
         \x20       version(value: number): SchemaType<T> & T;\n\
         \x20   }\n\
         \x20   export function schema<T extends Record<string, unknown>>(fields: T, name?: string, base?: unknown): SchemaType<T> & T;\n\
         \x20   export const t: {\n\
         \x20       number(): any;\n\
         \x20       float64(): any;\n\
         \x20       float32(): any;\n\
         \x20       int8(): any;\n\
         \x20       uint8(): any;\n\
         \x20       int16(): any;\n\
         \x20       uint16(): any;\n\
         \x20       int32(): any;\n\
         \x20       uint32(): any;\n\
         \x20       int64(): any;\n\
         \x20       boolean(): any;\n\
         \x20       bool(): any;\n\
         \x20       string(): any;\n\
         \x20       map<T>(target: T): any;\n\
         \x20       ref<T>(target: T): any;\n\
         \x20       array<T>(target: T): any;\n\
         \x20       set<T>(target: T): any;\n\
         \x20       collection<T>(target: T): any;\n\
         \x20       [key: string]: any;\n\
         \x20   };\n\
         }\n",
    );
    out
}
