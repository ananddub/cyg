// Isolated decode test of the Rust-produced snapshot bytes.
import { Decoder, t, schema } from '/mnt/SSD/Devloper/pannel/colyseus/node_modules/@colyseus/schema/build/index.mjs';

// Schema layout must match the server encoder's field indices:
// Player: x=0, y=1, score=2 | GameState: players=0, tick=1, coinX=2, coinY=3.
const Player = schema({ x: t.number(), y: t.number(), score: t.number() }, 'Player');
const GameState = schema(
    { players: t.map(Player), tick: t.number(), coinX: t.number(), coinY: t.number() },
    'GameState',
);

// print metadata field order
const md = GameState[Symbol.metadata] ?? GameState.prototype.constructor[Symbol.metadata];
console.log('[test] GameState metadata:', JSON.stringify(md?.names ?? md));

// Byte layout mirrors the Rust encoder (colyseus_state.rs):
// [ADD players=map1][SWITCH map][ADD idx1 "p1" ref2][SWITCH player]
// [ADD x=15.0][ADD y=-3.25][SWITCH root][ADD tick=10]
// NOTE: @colyseus/schema's fork encodes float64 LITTLE-endian.
const bytes = new Uint8Array([
    0x80, 0x01, 0xFF, 0x01, 0x80, 0x01, 0xA2, 0x70, 0x31, 0x02, 0xFF, 0x02,
    0x80, 0xCB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2E, 0x40, // x = 15.0 (f64 LE)
    0x81, 0xCB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A, 0xC0, // y = -3.25 (f64 LE)
    0xFF, 0x00, 0x81, 0x0A,
]);

const state = new GameState();
const decoder = new Decoder(state);
decoder.decode(bytes);

console.log('[test] tick =', state.tick);
console.log('[test] players =', [...state.players.entries()].map(([k, p]) => `${k}: (${p.x}, ${p.y})`));
if (state.tick === 10 && state.players.get('p1')?.x === 15 && state.players.get('p1')?.y === -3.25) {
    console.log('[test] DECODE OK');
} else {
    console.log('[test] DECODE MISMATCH');
}
