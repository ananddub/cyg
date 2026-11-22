// The JavaScript decoder is gone: every schema frame is decoded by the shared
// `cyg-core-rs` wasm. These tests drive that real path with the captured
// fixture frames (the same bytes the Rust `live_parity` test uses).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import CYG from "../cyg.js";

const { Room, SchemaDef } = CYG;
const FIXTURES = new URL("../../../../crates/cyg-core-rs/tests/fixtures/", import.meta.url);

// `cyg.js` loads the core next to itself in a browser; in Node the loader
// falls back to `fetch("cyg_core_rs.wasm")`, which has no filesystem root.
globalThis.fetch = async (url) => {
    const path = String(url) === "cyg_core_rs.wasm"
        ? new URL("../cyg_core_rs.wasm", import.meta.url)
        : new URL(String(url));
    return { arrayBuffer: async () => readFileSync(fileURLToPath(path)) };
};

function readFrames(bytes) {
    const frames = [];
    let at = 0;
    while (at + 4 <= bytes.length) {
        const len = new DataView(bytes.buffer, bytes.byteOffset + at, 4).getUint32(0, true);
        at += 4;
        if (at + len > bytes.length) break;
        frames.push(bytes.subarray(at, at + len));
        at += len;
    }
    return frames;
}

/** A Room with the host descriptor attached and the core loaded. */
async function attachedRoom() {
    const room = new Room("ws://127.0.0.1:7777", "fixture_room");
    const errors = [];
    room.on("schemaerror", (message) => errors.push(message));
    const descriptor = readFileSync(fileURLToPath(new URL("descriptor.bin", FIXTURES)));
    room._attachDescriptor(new Uint8Array(descriptor));
    await room._corePromise;
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(room._schemaError, null, `descriptor attach failed: ${room._schemaError}`);
    return { room, errors };
}

// Hash must match the AssemblyScript encoder for the same field list.
{
    const player = new SchemaDef("Player", 1).f64("x").f64("y").u32("flags");
    assert.equal(player.hash, 0x44d746fc, "schema hash parity with SchemaDef.ts");
}

// The packed fixture hydrates room.state through the wasm core.
{
    const { room, errors } = await attachedRoom();
    assert.equal(room.schema.name, "Player");
    const frames = readFrames(readFileSync(fileURLToPath(new URL("payloads.bin", FIXTURES))));
    let applied = 0;
    for (const frame of frames) {
        // The capture also holds a text control frame and a descriptor frame;
        // only OXS2 deltas belong to the schema lane.
        if (frame[0] !== 0xA5 && frame[0] !== 0x11) continue;
        const before = room._schemaReady;
        room._applySchemaViaCore(frame);
        if (!before && room._schemaReady) applied++;
    }
    assert.deepEqual(errors, [], "no frame may be rejected");
    assert.ok(applied > 0, "a full keyframe must mark the schema ready");
    assert.equal(room._schemaReady, true);
    assert.equal(room.state.size, 1);
    const player = room.state.get(9);
    assert.ok(player, "the captured player survives");
    assert.equal(player.pid, 9);
    assert.equal(player.name, "box 9");
    assert.equal(player.x, 120);
    assert.ok(Math.abs(player.y - 332.0088806152344) < 1e-6, `y ${player.y}`);
    assert.equal(player.grounded, false);
}

// A volatile-only frame rides its own tick-gated channel and still applies
// through the same packed decoder.
{
    const { room } = await attachedRoom();
    const frames = readFrames(readFileSync(fileURLToPath(new URL("payloads.bin", FIXTURES))));
    const full = frames.find((frame) =>
        frame.length > 20 &&
        frame[0] === 0x11 && frame[1] === 0xA5 && frame[2] === 0x64 &&
        frame[15] === 0x4F && frame[16] === 0x58 && frame[17] === 0x53 && frame[18] === 0x32);
    assert.ok(full, "fixture has a packed full frame");
    const body = full.subarray(1); // drop the WS opcode, keep [0xA5 "delta"]...
    const volatile = new Uint8Array(body.length + 5);
    volatile[0] = 0xA6;
    new DataView(volatile.buffer).setUint32(1, 4242, true);
    volatile.set(body, 5);
    room._handleInboundBinary(volatile);
    assert.equal(room.lastVolatileTick, 4242);
    assert.equal(room.state.size, 1);

    // A late, older volatile frame is dropped without touching the ledger.
    const older = volatile.slice();
    new DataView(older.buffer).setUint32(1, 4241, true);
    room._handleInboundBinary(older);
    assert.equal(room.lastVolatileTick, 4242);
    assert.equal(room.state.size, 1);

    // Native UDP fragments a large volatile frame through the delta
    // reassembler; the reassembled body keeps its 0xA6 header.
    const fragmented = new Uint8Array(body.length + 5);
    fragmented[0] = 0xA6;
    new DataView(fragmented.buffer).setUint32(1, 4243, true);
    fragmented.set(body, 5);
    room._processDeltaPayload(fragmented);
    assert.equal(room.lastVolatileTick, 4243);
    assert.equal(room.state.size, 1);

    // WebRTC splits a large volatile frame into 0xA7 datagrams; out-of-order
    // arrival must still assemble before the tick gate applies it.
    const big = new Uint8Array(body.length + 5);
    big[0] = 0xA6;
    new DataView(big.buffer).setUint32(1, 4245, true);
    big.set(body, 5);
    const chunks = [];
    for (let at = 0; at < big.length; at += 17) chunks.push(big.slice(at, at + 17));
    const fragments = chunks.map((chunk, index) => {
        const frame = new Uint8Array(7 + chunk.length);
        frame[0] = 0xA7;
        new DataView(frame.buffer).setUint32(1, 4245, true);
        frame[5] = index;
        frame[6] = chunks.length;
        frame.set(chunk, 7);
        return frame;
    });
    for (const frame of fragments.reverse()) room._handleInboundBinary(frame);
    assert.equal(room.lastVolatileTick, 4245);
    assert.equal(room.state.size, 1);
}

// The fixed-width wire is gone: a kind-1 section is a client error, not state.
{
    const { room, errors } = await attachedRoom();
    // [0xA5 "delta"][your_id i64][OXS2 kind=1 ...][acked u16][tick u32][stamp u16]
    const fixedWidth = new Uint8Array(6 + 8 + 13 + 8);
    fixedWidth.set([0xA5, 0x64, 0x65, 0x6C, 0x74, 0x61]);
    fixedWidth.set([0x4f, 0x58, 0x53, 0x32, 1], 14); // OXS2, the old kind 1
    room._applySchemaViaCore(fixedWidth);
    assert.equal(room.state.size, 0);
    assert.match(room._schemaError || "", /bad schema frame/);
    assert.ok(errors.length > 0, "schemaerror must fire");
}

console.log("web schema/core tests OK");

// Flattened child-schema fields stay on the wire but surface as `stats.kills`.
{
    const def = new SchemaDef("Player", 1).u32("stats__kills").u32("stats__deaths");
    def.fields[0].nested = "stats";
    def.fields[1].nested = "stats";
    def._rehash();
    const room = new Room("ws://127.0.0.1:7777", "nested_room");
    const entity = { id: 7, stats__kills: 3, stats__deaths: 1 };
    room._defineNestedViews(entity, def);
    assert.equal(entity.stats.kills, 3);
    entity.stats.kills = 9;
    assert.equal(entity.stats__kills, 9, "nested setter writes the flat wire field");
}

// Child-collection rows fold into parent.items by the packed (owner, slot) id.
{
    const room = new Room("ws://127.0.0.1:7777", "nested_array_room");
    const childId = (7 << 16) | 1;
    const parent = { id: 7 };
    const child = { id: childId, qty: 5 };
    room._nested = [{ collection: "players_items", parent: "players", field: "items" }];
    room._collections = [
        { name: "players", map: new Map([[7, parent]]) },
        { name: "players_items", map: new Map([[childId, child]]) }
    ];
    room._rebuildNested();
    assert.equal(parent.items[1].qty, 5);
    assert.equal(parent.items[1], child, "existing child object is kept");
}

// Old descriptors are rejected loudly instead of being partially decoded.
{
    const descriptor = new Uint8Array(
        readFileSync(fileURLToPath(new URL("descriptor.bin", FIXTURES))));
    descriptor[4] = 3;
    const room = new Room("ws://127.0.0.1:7777", "old_descriptor");
    const errors = [];
    room.on("schemaerror", (message) => errors.push(message));
    room._attachDescriptor(descriptor);
    assert.match(errors[0] || "", /unsupported schema descriptor v3/);
    assert.match(errors[0] || "", /Rebuild client and server together/);
}
