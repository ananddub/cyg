import legacy from "./core/legacy.js";
import type { SchemaDef, SchemaField } from "./types/legacy";
import type { CygSchemaTableDef } from "./types/generated";

/** WireKind discriminant -> legacy SchemaDef builder method name. */
const KIND_TO_BUILDER: Record<string, string | null> = {
    u8: "u8", i8: "i8", u16: "u16", i16: "i16", u32: "u32", i32: "i32",
    f32: "f32", f64: "f64", bool: "bool", i64: "i64",
    // string and array fields have no legacy builder; attach via `field()` if
    // the runtime supports them, otherwise skip (wasm core handles the bytes).
    string: null, array: null, none: null,
};

/**
 * Build a legacy runtime `SchemaDef` from a generated schema table
 * (`cyg_schema_generated.d.ts` → `{ name, version, fields }`).
 */
export function schemaFromTable(table: CygSchemaTableDef): SchemaDef {
    const def = new legacy.SchemaDef(table.name, table.version);
    for (const field of table.fields) {
        const builder = KIND_TO_BUILDER[field.kind];
        if (builder) {
            (def[builder as keyof typeof legacy.SchemaDef.prototype] as (name: string) => unknown).call(def, field.name);
        } else {
            // Unknown-to-builder kinds: push the raw metadata so field indices
            // stay aligned with the descriptor the server negotiated.
            (def.fields as unknown as SchemaField[]).push({
                name: field.name,
                type: field.kind,
                flags: field.flags,
            } as unknown as SchemaField);
        }
    }
    return def;
}

/** Attach a generated schema table to a room: `room.useSchema(schemaFromTable(StateSchema))`. */
export function useSchemaTable(
    room: { useSchema: (schema: SchemaDef) => unknown },
    table: CygSchemaTableDef,
): void {
    room.useSchema(schemaFromTable(table));
}
