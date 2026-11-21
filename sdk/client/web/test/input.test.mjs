import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import CYG from "../cyg.js";

const { Room, SchemaDef } = CYG;

globalThis.fetch = async (url) => {
    const path = String(url) === "cyg_core_rs.wasm"
        ? new URL("../cyg_core_rs.wasm", import.meta.url)
        : new URL(String(url));
    return { arrayBuffer: async () => readFileSync(fileURLToPath(path)) };
};

function u8(v) { return new Uint8Array([v & 0xff]); }
function u16(v) { const b = new Uint8Array(2); new DataView(b.buffer).setUint16(0, v, true); return b; }
function u32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); return b; }
function str(text) { const b = new TextEncoder().encode(text); return new Uint8Array([...u16(b.length), ...b]); }

/** A v2 OXSD with one `Input` collection, matching the declared fields. */
function inputDescriptor() {
    const parts = [
        new TextEncoder().encode("OXSD"), u8(2), u8(0), u16(1),
        str("Input"), u32(1), u32(0), u16(4),
        str("moveX"), u8(5), // f32
        str("moveZ"), u8(5),
        str("jump"), u8(7),  // bool
        str("skill"), u8(1), // u8
        str("State"), u32(0), u32(0), u16(0), u16(1),
        str("inputs"), u16(0),
    ];
    const length = parts.reduce((n, p) => n + p.length, 0);
    const out = new Uint8Array(length);
    let at = 0;
    for (const p of parts) { out.set(p, at); at += p.length; }
    return out;
}

async function coreInputRoom() {
    const room = new Room("ws://127.0.0.1:1", "input_room");
    room._attachDescriptor(inputDescriptor());
    await room._corePromise;
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(room._schemaError, null, `descriptor attach failed: ${room._schemaError}`);
    return room;
}

function schemaInputRoom() {
    const room = Object.create(Room.prototype);
    room.isSpectator = false;
    room.inputSeq = 0;
    room.bytesOut = 0;
    room.ws = { readyState: 1, send() { return true; } };
    room.serverChUnreliable = null;
    room.serverChReliable = null;
    room.serverRtcActive = false;
    const def = new SchemaDef("Input", 1);
    def._add("moveX", "f32");
    def._add("moveZ", "f32");
    def._add("jump", "bool");
    def._add("skill", "u8");
    room._inputSchema = def;
    return room;
}

// The packed body mirrors the generated `decodeInput` field-for-field:
// [seq u16 LE][f32 moveX][f32 moveZ][bool jump][u8 skill].
{
    const room = await coreInputRoom();
    const bytes = room._packSchemaInput({ moveX: 1.5, jump: true, skill: 7 });
    assert.equal(bytes.length, 12, "2 seq + 4 + 4 + 1 + 1 field bytes");
    const view = new DataView(bytes.buffer);
    assert.equal(view.getUint16(0, true), 1, "first frame is seq 1");
    assert.equal(view.getFloat32(2, true), 1.5);
    assert.equal(view.getFloat32(6, true), 0, "missing field packs its default");
    assert.equal(bytes[10], 1, "jump true");
    assert.equal(bytes[11], 7, "skill u8");

    const second = room._packSchemaInput({});
    assert.equal(new DataView(second.buffer).getUint16(0, true), 2, "seq increments");
}

// No declared Input def → the object lane is inert.
{
    const room = schemaInputRoom();
    room._inputSchema = null;
    assert.equal(room._packSchemaInput({ moveX: 1 }), null);
}

// `sendInput` takes exactly one thing: the input object. The old positional
// (flags, aimYaw, …) form is gone — it throws with a pointer to the fix.
{
    const room = schemaInputRoom();
    let seen = null;
    room._sendSchemaInput = (obj) => { seen = obj; return true; };
    room.sendInput({ moveX: 1, moveZ: -1 });
    assert.deepEqual(seen, { moveX: 1, moveZ: -1 });

    assert.throws(() => room.sendInput(1, 0, 0, 1, 0), TypeError);
    assert.throws(() => room.sendInput(1, 0, 0, 1, 0), /Input.*schema/);
}

// The keyboard handle fills the declared fields by name.
{
    const room = schemaInputRoom();
    const input = room.input();
    let sent = null;
    room._sendSchemaInput = (obj) => { sent = obj; return true; };
    input.data.right = true;
    input.data.jump = true;
    input.send();
    assert.ok(sent, "handle.send() used the schema lane");
    assert.equal(sent.moveX, 1, "right key derives moveX");
    assert.equal(sent.moveZ, 0);
    assert.equal(sent.jump, true);
    assert.equal(sent.skill, 0, "undeclared-in-handle field packs its default");
}

// Without a declared Input def the handle says so (once) instead of packing.
{
    const room = schemaInputRoom();
    room._inputSchema = null;
    const warnings = [];
    const originalWarn = console.warn;
    console.warn = (message) => warnings.push(message);
    try {
        assert.equal(room.input().send(), false);
        assert.equal(room.input().send(), false);
    } finally {
        console.warn = originalWarn;
    }
    assert.equal(warnings.length, 1, "warns exactly once");
    assert.match(warnings[0] || "", /Input/);
}

console.log("input.test: all assertions passed");
