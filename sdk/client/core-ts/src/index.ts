// @cyg/core-ts — typesafe entry point.
//
// The binary codec lives in `cyg-core-rs.wasm` (single Rust core, also
// embedded natively by Unity/Godot/Unreal). This package adds the typesafe
// TypeScript surface on top: transports, room lifecycle, voice, REST and a
// typed `Room` driven by the generated schema bindings.

import engine from "./core/engine.js";

// Typed surface
import { TypedRoom } from "./room.js";
import { schemaFromTable, useSchemaTable } from "./schema.js";
export { TypedRoom, type RoomContract } from "./room.js";
export { schemaFromTable, useSchemaTable } from "./schema.js";
export type {
    CygSchemaFieldDef,
    CygSchemaTableDef,
} from "./types/generated.js";

// Core surface (Room, Client, Voice, prediction, wire codecs) — verbatim
// behavior, typed by src/types/engine.d.ts until the class-by-class module
// split lands. The default export is flat core parity (`CYG.Room`, `CYG.Client`,
// ...) plus the typed additions, so the UMD bundle is a drop-in replacement
// for sdk/client/web/cyg.js.
export const cyg = engine;
export default {
    ...engine,
    TypedRoom,
    schemaFromTable,
    useSchemaTable,
};
