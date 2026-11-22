import assert from "node:assert/strict";
import test from "node:test";
import CYG from "../dist/cyg.cjs";

const { TypedRoom, schemaFromTable, useSchemaTable, Room, SchemaDef, encodeInputFrame } = CYG;

test("flat core surface is preserved", () => {
    assert.equal(typeof Room, "function");
    assert.equal(typeof SchemaDef, "function");
    assert.equal(typeof encodeInputFrame, "function");
    assert.equal(typeof CYG.version, "string");
});

test("typed room wraps the engine room", () => {
    const room = new TypedRoom("http://127.0.0.1:19224", "arena");
    assert.equal(room.isOpen, false);
    assert.equal(room.playerId, null);
    assert.equal(typeof room.connect, "function");
    assert.equal(typeof room.onStateChange, "function");
});

test("schemaFromTable builds a runtime SchemaDef from generated tables", () => {
    // Table shape matches the engine's `--lang ts` codegen output.
    const table = {
        name: "Player",
        version: 3,
        hash: 42,
        fields: [
            { name: "x", kind: "f64", code: 8, flags: 0, element: "none", size: 8 },
            { name: "hp", kind: "u8", code: 1, flags: 0, element: "none", size: 1 },
            { name: "target", kind: "u32", code: 3, flags: 1, element: "none", size: 4 },
        ],
    };
    const def = schemaFromTable(table);
    assert.equal(def.name, "Player");
    assert.equal(def.version, 3);
    assert.ok(def.fields.length >= 2, "builder-backed fields are registered");
    assert.equal(def.indexOf("x"), 0);
    assert.equal(def.indexOf("hp"), 1);
});

test("useSchemaTable attaches a schema to a room", () => {
    const room = new Room("http://127.0.0.1:19224", "arena");
    const table = {
        name: "State",
        version: 1,
        hash: 7,
        fields: [{ name: "round", kind: "u16", code: 2, flags: 0, element: "none", size: 2 }],
    };
    assert.doesNotThrow(() => useSchemaTable(room, table));
});
