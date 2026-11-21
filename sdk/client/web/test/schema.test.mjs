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

// The fixed-width wire is gone: a kind-1 section is a client error, not state.
{
    const { room, errors } = await attachedRoom();
    // [0xA5 "delta"][your_id i64][OXS2 kind=1 ...][acked u16][tick u32][stamp u16]
    const legacy = new Uint8Array(6 + 8 + 13 + 8);
    legacy.set([0xA5, 0x64, 0x65, 0x6C, 0x74, 0x61]);
    legacy.set([0x4f, 0x58, 0x53, 0x32, 1], 14); // OXS2, the old kind 1
    room._applySchemaViaCore(legacy);
    assert.equal(room.state.size, 0);
    assert.match(room._schemaError || "", /bad schema frame/);
    assert.ok(errors.length > 0, "schemaerror must fire");
}

console.log("web schema/core tests OK");
