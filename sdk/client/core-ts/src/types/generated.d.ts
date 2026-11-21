/**
 * Shape of the runtime schema tables emitted by the engine's TypeScript
 * codegen (`ox-game schema-codegen . --lang ts` → cyg_schema_generated.d.ts).
 *
 * This file declares the *contract*; games get the concrete tables generated
 * from their own schema source and pass them to `schemaFromTable()`.
 */
export interface CygSchemaFieldDef {
    name: string;
    kind: string;
    code: number;
    flags: number;
    element: string;
    size: number;
}

export interface CygSchemaTableDef {
    name: string;
    version: number;
    hash: number;
    fields: readonly CygSchemaFieldDef[];
}
