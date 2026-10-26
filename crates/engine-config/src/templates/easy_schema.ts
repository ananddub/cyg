// Network state contract — the only schema source in this project.
//
// `ox-game run` compiles this file before starting the room:
//   • cyg.schema.ts  — typed AssemblyScript class imported by room.ts
//   • cyg.schema.bin — descriptor delivered to every client on join
//
// The browser needs no matching schema file. It rebuilds the same field list
// from the descriptor, exactly like Colyseus.
//
// Syntax is Colyseus v5: https://docs.colyseus.io/state/schema/
import { schema, t } from "@colyseus/schema";

// One player entity. Position is a double on the wire; flags is the held-key mask.
export const Player = schema({
    x: t.number().default(0),
    y: t.number().default(0),
    flags: t.uint32().default(0),
}, "Player");

// Root state. Every `t.map(Player)` entry is synced by numeric entity id.
export const State = schema({
    players: t.map(Player),
}, "State");
