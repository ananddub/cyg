// Network state contract — the only schema source in this project.
//
// `cyg-game run` compiles this file before starting the room:
//   • cyg.schema.ts  — typed AssemblyScript class imported by the room
//   • cyg.schema.bin — descriptor delivered to every client on join
//
// The browser needs no matching schema file. It rebuilds the same field list
// from the descriptor.
//
// CYG schema DSL.
import { schema, t } from "cyg";

// One player entity. Position is a double on the wire; name is whatever the
// client sent in its join options.
export const Player = schema({
    pid: t.int64().default(0),
    x: t.number().default(0),
    y: t.number().default(0),
    name: t.string(),
}, "Player");

// Root state. Every `t.map(Player)` entry is synced by numeric entity id.
export const PlayerSchema = Player;
export const State = schema({
    players: t.map(Player),
}, "State");

// Typed join options — declare YOUR fields; the client sends
// `joinOrCreate(room, { name, team })` and onJoin receives a typed
// `options` (no JSON parsing, no jsonString needed). Strings OK here.
export const JoinOptions = schema({
    name: t.string(),
    team: t.uint8().default(0),
}, "JoinOptions");

// Per-tick input frame — declare YOUR fields (numeric/bool, fixed-size).
// The client sends `room.sendInput({ moveX, moveZ, jump })` and the room
// receives them typed via `decodeInput` — neither side packs a byte; the
// field order below is the wire order.
export const Input = schema({
    moveX: t.f32().default(0),
    moveZ: t.f32().default(0),
    jump: t.boolean().default(false),
}, "Input");
