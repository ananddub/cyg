// Isolated decode test of the Rust-produced snapshot bytes.
import { Decoder, t, schema } from '/mnt/SSD/Devloper/pannel/colyseus/node_modules/@colyseus/schema/build/index.mjs';

const Player = schema({ x: t.number(), y: t.number() }, 'Player');
const GameState = schema({ players: t.map(Player), tick: t.number() }, 'GameState');

// print metadata field order
const md = GameState[Symbol.metadata] ?? GameState.prototype.constructor[Symbol.metadata];
console.log('[test] GameState metadata:', JSON.stringify(md?.names ?? md));

const bytes = new Uint8Array([
    0x80, 0x01, 0xFF, 0x01, 0x80, 0x01, 0xA2, 0x70, 0x31, 0x02, 0xFF, 0x02,
    0x80, 0xCB, 0x40, 0x2F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x81, 0xCB, 0x3F, 0xEE, 0xA2, 0xA6, 0xCB, 0x2D, 0x40, 0xA8,
    0xFF, 0x00, 0x81, 0x0A,
]);

const state = new GameState();
const decoder = new Decoder(state);
decoder.decode(bytes);

console.log('[test] tick =', state.tick);
console.log('[test] players =', [...state.players.entries()].map(([k, p]) => `${k}: (${p.x}, ${p.y})`));
if (state.tick === 10 && state.players.get('p1')?.x === 15) {
    console.log('[test] DECODE OK');
} else {
    console.log('[test] DECODE MISMATCH');
}
