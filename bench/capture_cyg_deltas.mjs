/**
 * Capture real server->client delta bytes + the cyg.js decoded state, as
 * parity fixtures for crates/cyg-core-rs/tests/live_parity.rs.
 *
 *   node bench/capture_cyg_deltas.mjs --url http://127.0.0.1:8297 \
 *       --room arena --seconds 10 --out crates/cyg-core-rs/tests/fixtures
 *
 * Writes:
 *   descriptor.bin    the room's cyg.schema.bin copy
 *   payloads.bin      length-prefixed u32-LE raw WS messages (post-transport)
 *   expected.json     cyg.js's own decoded state at capture end
 */
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const CYG = require(path.resolve(here, '..', 'sdk', 'client', 'web', 'cyg.js'));

const argv = process.argv.slice(2);
const arg = (name, def) => {
    const i = argv.indexOf(`--${name}`);
    return i >= 0 ? argv[i + 1] : def;
};
const URL_ = arg('url', 'http://127.0.0.1:8297');
const ROOM = arg('room', 'arena');
const SECONDS = Number(arg('seconds', 10));
const OUT = arg('out', 'crates/cyg-core-rs/tests/fixtures');

const client = new CYG.Client(URL_);
const room = await client.joinOrCreate(ROOM, { autoReconnect: false });

const ws = room.ws; // the game-port WebSocket cyg.js opened
const messages = [];
ws.addEventListener('message', (ev) => {
    const data = ev.data;
    if (data instanceof ArrayBuffer) {
        messages.push(new Uint8Array(data));
    } else if (typeof data === 'string') {
        messages.push(new TextEncoder().encode(data));
    } else if (ArrayBuffer.isView(data)) {
        messages.push(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    }
});

// descriptor comes either pushed by the host or read from the build output;
// capture whatever the room got.
await new Promise((r) => setTimeout(r, 2000));
if (!room.schema) {
    console.error('no schema descriptor received from host');
    process.exit(1);
}
const descriptor = room.schemaDescriptor || room.schema?.descriptor;
let descriptorBytes = null;
if (descriptor instanceof Uint8Array) {
    descriptorBytes = descriptor;
} else {
    // fall back to the build output the server loaded
    descriptorBytes = fs.readFileSync(path.resolve(here, '..', 'bench-game-2donly', 'out', 'cyg.schema.bin'));
}
console.error(`descriptor: ${descriptorBytes.length} bytes`);

await new Promise((r) => setTimeout(r, SECONDS * 1000));

// cyg.js decoded state — the ground truth the Rust core must match
const state = room.state;
const players = [];
const src = state.players ?? state.entities ?? null;
if (src && typeof src.forEach === 'function') {
    src.forEach((p, id) => {
        const row = { id };
        for (const key of Object.keys(p)) {
            const v = p[key];
            if (typeof v === 'number' || typeof v === 'boolean' || typeof v === 'string') row[key] = v;
        }
        players.push(row);
    });
}

fs.mkdirSync(OUT, { recursive: true });
fs.writeFileSync(path.join(OUT, 'descriptor.bin'), descriptorBytes);
const lp = Buffer.alloc(4);
const blobs = messages.map((m) => {
    const buf = Buffer.concat([(() => { lp.writeUInt32LE(m.length); return lp; })(), m]);
    return buf;
});
fs.writeFileSync(path.join(OUT, 'payloads.bin'), Buffer.concat(blobs));
fs.writeFileSync(path.join(OUT, 'expected.json'), JSON.stringify({
    capturedMessages: messages.length,
    players,
}, null, 2));
console.error(`captured ${messages.length} messages, ${players.length} players`);
console.error(JSON.stringify(players.slice(0, 2)));
try { room.leave(true); } catch (_) {}
process.exit(0);
