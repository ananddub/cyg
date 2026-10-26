import assert from "node:assert/strict";
import CYG from "../cyg.js";

const { Room, SchemaDef, Callbacks } = CYG;

const MAGIC = [0x4f, 0x58, 0x53, 0x32]; // OXS2
const DESCRIPTOR_MAGIC = [0x4f, 0x58, 0x53, 0x44]; // OXSD

function concat(parts) {
    const length = parts.reduce((n, p) => n + p.length, 0);
    const out = new Uint8Array(length);
    let at = 0;
    for (const p of parts) {
        out.set(p, at);
        at += p.length;
    }
    return out;
}

function u8(value) {
    return new Uint8Array([value & 0xff]);
}

function u32(value) {
    const out = new Uint8Array(4);
    new DataView(out.buffer).setUint32(0, value >>> 0, true);
    return out;
}

function u16(value) {
    const out = new Uint8Array(2);
    new DataView(out.buffer).setUint16(0, value & 0xffff, true);
    return out;
}

function descriptorString(text) {
    const bytes = new TextEncoder().encode(text);
    return concat([u16(bytes.length), bytes]);
}

/** Build the same "OXSD" layout engine-schema emits, so clients can be tested offline. */
function descriptor(schemas, rootName, rootIndex) {
    const parts = [new Uint8Array(DESCRIPTOR_MAGIC), u8(1), u8(0), u16(schemas.length)];
    for (const schema of schemas) {
        parts.push(descriptorString(schema.name), u32(schema.version || 1), u32(0), u16(schema.fields.length));
        for (const field of schema.fields) parts.push(descriptorString(field.name), u8(field.code));
    }
    parts.push(descriptorString(rootName), u16(rootIndex));
    return concat(parts);
}

function f32(value) {
    const out = new Uint8Array(4);
    new DataView(out.buffer).setFloat32(0, value, true);
    return out;
}

function f64(value) {
    const out = new Uint8Array(8);
    new DataView(out.buffer).setFloat64(0, value, true);
    return out;
}

function str(value) {
    const bytes = new TextEncoder().encode(value);
    return concat([u8(bytes.length), bytes]);
}

function optionalF64(value) {
    return value === null ? u8(0) : concat([u8(1), f64(value)]);
}

function f32Array(values) {
    return concat([u8(values.length), ...values.map(f32)]);
}

function section(kind, hash, body) {
    return concat([new Uint8Array(MAGIC), u8(kind), u32(hash), body]);
}

function fullSection(hash, entities) {
    const parts = [u32(entities.length)];
    for (const [id, x, y, flags] of entities) {
        parts.push(u32(id), f64(x), f64(y), u32(flags));
    }
    return section(1, hash, concat(parts));
}

function patchSection(hash, ops) {
    const parts = [u32(ops.length)];
    for (const op of ops) {
        if (op.type === "add") {
            parts.push(u8(1), u32(op.id), f64(op.x), f64(op.y), u32(op.flags));
        } else if (op.type === "remove") {
            parts.push(u8(2), u32(op.id));
        } else if (op.type === "update") {
            let mask = 0;
            const values = [];
            if (op.x !== undefined) { mask |= 1; values.push(f64(op.x)); }
            if (op.y !== undefined) { mask |= 2; values.push(f64(op.y)); }
            if (op.flags !== undefined) { mask |= 4; values.push(u32(op.flags)); }
            parts.push(u8(3), u32(op.id), u32(mask), ...values);
        }
    }
    return section(2, hash, concat(parts));
}

/** v2 descriptor: root scalars + any number of collections. */
function descriptorV2(schemas, root) {
    const parts = [new Uint8Array(DESCRIPTOR_MAGIC), u8(2), u8(0), u16(schemas.length)];
    for (const schema of schemas) {
        parts.push(descriptorString(schema.name), u32(schema.version || 1), u32(0), u16(schema.fields.length));
        for (const field of schema.fields) parts.push(descriptorString(field.name), u8(field.code));
    }
    parts.push(
        descriptorString(root.name),
        u32(root.version || 1),
        u32(0),
        u16(root.scalars.length)
    );
    for (const field of root.scalars) parts.push(descriptorString(field.name), u8(field.code));
    parts.push(u16(root.collections.length));
    for (const collection of root.collections) {
        parts.push(descriptorString(collection.name), u16(collection.index));
    }
    return concat(parts);
}

function worldFullSection(rootHash, rootValues, collections) {
    const parts = [u8(3), u32(rootHash), u16(rootValues.length)];
    for (const value of rootValues) parts.push(value);
    parts.push(u16(collections.length));
    for (const collection of collections) {
        parts.push(u16(collection.id), u32(collection.hash), u32(collection.section.length), collection.section);
    }
    return concat([new Uint8Array(MAGIC), ...parts]);
}

/** v3 descriptor: per-field flags and array element codes. */
function descriptorV3(schemas, root) {
    const parts = [new Uint8Array(DESCRIPTOR_MAGIC), u8(3), u8(0), u16(schemas.length)];
    const writeField = (field) => {
        parts.push(
            descriptorString(field.name),
            u8(field.code),
            u8(field.flags || 0),
            u8(field.element || 0)
        );
    };
    for (const schema of schemas) {
        parts.push(descriptorString(schema.name), u32(schema.version || 1), u32(0), u16(schema.fields.length));
        for (const field of schema.fields) writeField(field);
    }
    parts.push(descriptorString(root.name), u32(root.version || 1), u32(0), u16(root.scalars.length));
    for (const field of root.scalars) writeField(field);
    parts.push(u16(root.collections.length));
    for (const collection of root.collections) {
        parts.push(descriptorString(collection.name), u16(collection.index));
    }
    return concat(parts);
}

function worldPatchSection(rootHash, scalarCount, mask, rootValues, collections) {
    const parts = [u8(4), u32(rootHash), u16(scalarCount), u32(mask)];
    for (const value of rootValues) parts.push(value);
    parts.push(u16(collections.length));
    for (const collection of collections) {
        parts.push(u16(collection.id), u32(collection.hash), u32(collection.section.length), collection.section);
    }
    return concat([new Uint8Array(MAGIC), ...parts]);
}

function makeRoom() {
    const room = new Room("ws://127.0.0.1:7777", "test_room");
    const player = new SchemaDef("Player", 1).f64("x").f64("y").u32("flags");
    room.useSchema(player);
    return { room, player };
}

// Hash must match the AssemblyScript encoder for the same field list.
{
    const player = new SchemaDef("Player", 1).f64("x").f64("y").u32("flags");
    assert.equal(player.hash, 0x44d746fc, "schema hash parity with SchemaDef.ts");
}

// Full snapshot adds entities; a patch mutates and removes them.
{
    const { room, player } = makeRoom();
    const added = [];
    const changed = [];
    const removed = [];
    room.onAdd((entity, id) => added.push([id, entity]));
    room.onChange((entity, field, value) => changed.push([entity.id, field, value]));
    room.onRemove((entity, id) => removed.push([id, entity]));

    room._applySchema(fullSection(player.hash, [
        [1, 10, 20, 0],
        [2, 30, 40, 1]
    ]));
    assert.equal(room.state.size, 2);
    assert.deepEqual([room.state.get(2).x, room.state.get(2).y, room.state.get(2).flags], [30, 40, 1]);
    assert.equal(added.length, 2);

    room._applySchema(patchSection(player.hash, [
        { type: "update", id: 1, x: 11.5, flags: 9 },
        { type: "remove", id: 2 }
    ]));
    assert.equal(room.state.size, 1);
    assert.equal(room.state.get(1).x, 11.5);
    assert.equal(room.state.get(1).flags, 9);
    assert.equal(room.state.get(1).y, 20, "untouched fields survive a patch");
    assert.deepEqual(changed, [[1, "x", 11.5], [1, "flags", 9]]);
    assert.equal(removed.length, 1);
    assert.equal(removed[0][0], 2);
}

// A patch that races the full snapshot is ignored; the host keeps sending full.
{
    const { room, player } = makeRoom();
    room._applySchema(patchSection(player.hash, [
        { type: "add", id: 7, x: 1, y: 2, flags: 0 }
    ]));
    assert.equal(room.state.size, 0, "patch before full must not create a baseline");
    assert.equal(room._schemaReady, false);
}

// Mismatched client/server schemas surface a clear error instead of garbage.
{
    const { room } = makeRoom();
    let error = null;
    room.on("schemaerror", (message) => { error = message; });
    room._applySchema(fullSection(0xdeadbeef, []));
    assert.match(error, /schema hash mismatch/);
    assert.equal(room.state.size, 0);
}

// Multiple fields with the same type keep their declared order.
{
    const schema = new SchemaDef("Vec", 2).f32("a").f32("b").bool("on").i64("big");
    const room = new Room("ws://127.0.0.1:7777", "vec_room");
    room.useSchema(schema);
    const body = concat([u32(1), u32(3), (() => {
        const b = new Uint8Array(4 + 4 + 1 + 8);
        const v = new DataView(b.buffer);
        v.setFloat32(0, 1.25, true);
        v.setFloat32(4, -2.5, true);
        v.setUint8(8, 1);
        v.setBigInt64(9, 123n, true);
        return b;
    })()]);
    room._applySchema(section(1, schema.hash, body));
    const entity = room.state.get(3);
    assert.equal(entity.a, 1.25);
    assert.equal(entity.b, -2.5);
    assert.equal(entity.on, true);
    assert.equal(entity.big, 123);
}

// The OXSD descriptor attaches the schema automatically and aliases the root
// collection, so `room.state.players` (Colyseus-style) is the entity map.
{
    const room = new Room("ws://127.0.0.1:7777", "auto_schema");
    room._handleInboundBinary(descriptor(
        [{ name: "Player", version: 1, fields: [
            { name: "x", code: 6 },
            { name: "y", code: 6 },
            { name: "flags", code: 3 }
        ] }],
        "players",
        0
    ));

    assert.equal(room.rootCollection, "players");
    assert.ok(room.schema, "descriptor builds the client schema");
    assert.equal(room.state.players, room.state, "root collection alias points at the same map");
    const manual = new SchemaDef("Player", 1).f64("x").f64("y").u32("flags");
    assert.equal(room.schema.hash, manual.hash, "descriptor hash matches a manual SchemaDef");

    // Colyseus v5 naming parity: getStateCallbacks(room) and Callbacks.get(room).
    assert.equal(typeof CYG.getStateCallbacks, "function");
    const aliasCallbacks = CYG.getStateCallbacks(room);
    assert.equal(typeof aliasCallbacks.onAdd, "function");
    assert.equal(typeof Callbacks.getStateCallbacks(room).listen, "function");

    const callbacks = Callbacks.get(room);
    const added = [];
    const fieldChanges = [];
    const itemChanges = [];
    callbacks.onAdd("players", (player, id) => {
        added.push(id);
        callbacks.listen(player, "x", (current, previous) => fieldChanges.push([id, previous, current]));
    });
    callbacks.onChange("players", (id) => itemChanges.push(id));

    room._applySchema(fullSection(room.schema.hash, [[1, 10, 20, 0]]));
    assert.deepEqual(added, [1]);
    assert.equal(added.length, 1);

    room._applySchema(patchSection(room.schema.hash, [{ type: "update", id: 1, x: 11.5 }]));
    assert.deepEqual(fieldChanges, [[1, 10, 11.5]], "listen() sees current and previous value");
    assert.deepEqual(itemChanges, [1], "collection onChange sees the changed entity");

    let entityChanges = 0;
    const entity = room.state.get(1);
    callbacks.onChange(entity, () => { entityChanges += 1; });
    room._applySchema(patchSection(room.schema.hash, [{ type: "update", id: 1, y: 21 }]));
    assert.equal(entityChanges, 1, "entity-level onChange fires once per patch");

    const view = {};
    callbacks.bindTo(room.state.get(1), view);
    assert.equal(view.x, 11.5, "bindTo copies current values");
    room._applySchema(patchSection(room.schema.hash, [{ type: "update", id: 1, x: 12 }]));
    assert.equal(view.x, 12, "bindTo follows future changes");

    const legacy = Callbacks.getLegacy(room);
    let legacyAdds = 0;
    legacy.players.onAdd(() => { legacyAdds += 1; }, false);
    room._applySchema(patchSection(room.schema.hash, [
        { type: "add", id: 2, x: 0, y: 0, flags: 0 }
    ]));
    assert.equal(legacyAdds, 1, "legacy $(state).players.onAdd still works");
}

// i8 / i16 fields round-trip through the descriptor and the wire reader.
{
    const room = new Room("ws://127.0.0.1:7777", "signed_schema");
    room._handleInboundBinary(descriptor(
        [{ name: "Signed", version: 1, fields: [
            { name: "small", code: 9 },
            { name: "medium", code: 10 }
        ] }],
        "entities",
        0
    ));
    const body = new Uint8Array(4 + 4 + 1 + 2);
    const view = new DataView(body.buffer);
    view.setUint32(0, 1, true);      // entity count
    view.setUint32(4, 42, true);     // id
    view.setInt8(8, -7);
    view.setInt16(9, -1234, true);
    room._applySchema(section(1, room.schema.hash, body));
    const entity = room.state.get(42);
    assert.equal(entity.small, -7);
    assert.equal(entity.medium, -1234);
}

// Colyseus immediate defaults: onAdd/listen/bindTo fire now, unless the
// registration happens inside an add/change dispatch.
{
    const { room, player } = makeRoom();
    room._applySchema(fullSection(player.hash, [[1, 10, 20, 0]]));
    const callbacks = Callbacks.get(room);

    const immediateAdds = [];
    const unsubscribe = callbacks.onAdd("players", (entity, id) => immediateAdds.push(id));
    assert.deepEqual(immediateAdds, [1], "onAdd() defaults to immediate");
    unsubscribe();

    const deferredAdds = [];
    callbacks.onAdd("players", (entity, id) => deferredAdds.push(id), false);
    assert.deepEqual(deferredAdds, [], "onAdd(..., false) does not enumerate existing rows");

    const entity = room.state.get(1);
    const seen = [];
    callbacks.listen(entity, "x", (current, previous) => seen.push([current, previous]));
    assert.deepEqual(seen, [[10, undefined]], "listen() defaults to immediate");

    const view = {};
    callbacks.bindTo(entity, view);
    assert.equal(view.x, 10, "bindTo() defaults to immediate");

    const deferredView = {};
    callbacks.bindTo(entity, deferredView, ["x"], false);
    assert.equal(deferredView.x, undefined, "bindTo(..., false) defers the first copy");
}

// Reconnect/full-snapshot reconciliation keeps entity identity, reports removed
// rows, and only fires field callbacks for values that actually changed.
{
    const { room, player } = makeRoom();
    const added = [];
    const removed = [];
    const fields = [];
    let updates = 0;
    room.onAdd((entity, id) => added.push(id));
    room.onRemove((entity, id) => removed.push(id));
    room.onStateChange(() => { updates += 1; });
    room._applySchema(fullSection(player.hash, [[1, 10, 20, 0], [2, 30, 40, 0]]));
    const first = room.state.get(1);
    const callbacks = Callbacks.get(room);
    callbacks.listen(first, "x", (current, previous) => fields.push([current, previous]), false);

    added.length = 0;
    room._applySchema(fullSection(player.hash, [[1, 99, 20, 0], [3, 50, 60, 0]]));

    assert.equal(room.state.get(1), first, "existing entity object survives a full snapshot");
    assert.equal(first.x, 99);
    assert.deepEqual(added, [3], "only genuinely new entities fire onAdd");
    assert.deepEqual(removed, [2], "rows missing from the new full snapshot fire onRemove");
    assert.deepEqual(fields, [[99, 10]], "listen() sees the reconciled previous value");
    assert.ok(updates >= 2, "each full snapshot still emits a state change");
}

// Multi-collection world (descriptor v2): root scalars + independent maps,
// with callbacks scoped to the collection they were registered on.
{
    const room = new Room("ws://127.0.0.1:7777", "world_schema");
    const player = new SchemaDef("Player", 1).f64("x").f64("y").u32("flags");
    const projectile = new SchemaDef("Projectile", 1).f64("x").u32("owner");
    const root = new SchemaDef("State", 1).u32("tick").bool("paused");

    room._handleInboundBinary(descriptorV2(
        [
            { name: "Player", version: 1, fields: [
                { name: "x", code: 6 }, { name: "y", code: 6 }, { name: "flags", code: 3 }
            ] },
            { name: "Projectile", version: 1, fields: [
                { name: "x", code: 6 }, { name: "owner", code: 3 }
            ] }
        ],
        {
            name: "State",
            version: 1,
            scalars: [{ name: "tick", code: 3 }, { name: "paused", code: 7 }],
            collections: [{ name: "players", index: 0 }, { name: "bullets", index: 1 }]
        }
    ));

    assert.equal(room.rootCollection, "players");
    assert.equal(room.state.players, room.state, "the first collection aliases room.state");
    assert.ok(room.state.bullets instanceof Map);
    assert.equal(room.state.bullets.size, 0);
    assert.equal(room.state.tick, 0, "root scalars start at their zero value");
    assert.equal(room.state.paused, false);

    const callbacks = Callbacks.get(room);
    const playerAdds = [];
    const bulletAdds = [];
    const tickChanges = [];
    const playerChanges = [];
    callbacks.onAdd("players", (entity, id) => playerAdds.push(id), false);
    callbacks.onAdd("bullets", (entity, id) => bulletAdds.push(id), false);
    callbacks.onChange("players", (id) => playerChanges.push(id));
    callbacks.listen(room.state, "tick", (current, previous) => tickChanges.push([current, previous]), false);

    room._applySchema(worldFullSection(root.hash, [u32(10), u8(0)], [
        { id: 0, hash: player.hash, section: fullSection(player.hash, [[1, 10, 20, 4]]) },
        { id: 1, hash: projectile.hash, section: fullSection(projectile.hash, [[7, 2.5, 1]]) }
    ]));

    assert.equal(room.state.players.size, 1);
    assert.equal(room.state.bullets.size, 1);
    assert.equal(room.state.players.get(1).x, 10);
    assert.equal(room.state.bullets.get(7).x, 2.5);
    assert.equal(room.state.tick, 10);
    assert.deepEqual(playerAdds, [1]);
    assert.deepEqual(bulletAdds, [7]);
    assert.equal(room.state.players._ready, true);
    assert.equal(room.state.bullets._ready, true);

    const immediate = [];
    callbacks.onAdd("bullets", (entity, id) => immediate.push(id));
    assert.deepEqual(immediate, [7], "immediate onAdd is scoped to the named collection");

    room._applySchema(worldPatchSection(root.hash, 2, 0b01, [u32(11)], [
        { id: 0, hash: player.hash, section: patchSection(player.hash, [{ type: "update", id: 1, x: 12 }]) }
    ]));
    assert.equal(room.state.tick, 11, "root scalar patch applies");
    assert.equal(room.state.players.get(1).x, 12);
    assert.deepEqual(tickChanges, [[10, 0], [11, 10]], "root scalar listeners get previous values");
    assert.deepEqual(playerChanges, [1], "only the changed collection fires");
    assert.deepEqual(bulletAdds, [7]);

    room._applySchema(worldPatchSection(root.hash, 2, 0, [], [
        { id: 1, hash: projectile.hash, section: patchSection(projectile.hash, [
            { type: "add", id: 8, x: 1, owner: 2 }
        ]) }
    ]));
    assert.equal(room.state.bullets.size, 2);
    assert.deepEqual(bulletAdds, [7, 8]);
    assert.deepEqual(playerChanges, [1], "another collection's patch leaves players alone");
}

// A multi-collection patch before the full baseline is ignored.
{
    const room = new Room("ws://127.0.0.1:7777", "world_patch_first");
    const player = new SchemaDef("Player", 1).f64("x");
    const root = new SchemaDef("State", 1).u32("tick");
    room._handleInboundBinary(descriptorV2(
        [{ name: "Player", version: 1, fields: [{ name: "x", code: 6 }] }],
        {
            name: "State",
            version: 1,
            scalars: [{ name: "tick", code: 3 }],
            collections: [{ name: "players", index: 0 }]
        }
    ));
    room._applySchema(worldPatchSection(root.hash, 1, 1, [u32(99)], []));
    assert.equal(room.state.tick, 0);
    assert.equal(room._schemaReady, false);
    assert.equal(room._schemaAcked, false);
}

// Root hash mismatches are reported instead of applying garbage.
{
    const room = new Room("ws://127.0.0.1:7777", "world_bad_root");
    const player = new SchemaDef("Player", 1).f64("x");
    room._handleInboundBinary(descriptorV2(
        [{ name: "Player", version: 1, fields: [{ name: "x", code: 6 }] }],
        {
            name: "State",
            version: 1,
            scalars: [{ name: "tick", code: 3 }],
            collections: [{ name: "players", index: 0 }]
        }
    ));
    let error = null;
    room.on("schemaerror", (message) => { error = message; });
    room._applySchema(worldFullSection(0xdeadbeef, [u32(1)], []));
    assert.match(error, /root schema hash mismatch/);
    assert.equal(room.state.tick, 0);
}

// Descriptor v3: strings, optional presence, numeric arrays, static/transient.
{
    const room = new Room("ws://127.0.0.1:7777", "phase3_schema");
    room._handleInboundBinary(descriptorV3(
        [{
            name: "Player",
            version: 1,
            fields: [
                { name: "name", code: 11 },
                { name: "hp", code: 2 },
                { name: "score", code: 6, flags: 1 },
                { name: "aim", code: 12, element: 5 },
                { name: "skin", code: 3, flags: 2 },
                { name: "blink", code: 1, flags: 4 }
            ]
        }],
        {
            name: "State",
            version: 1,
            scalars: [{ name: "title", code: 11 }],
            collections: [{ name: "players", index: 0 }]
        }
    ));

    const fields = room.schema.fields;
    assert.equal(fields[2].optional, true);
    assert.equal(fields[2].fullOnly, false);
    assert.equal(fields[3].type, "array");
    assert.equal(fields[3].element, "f32");
    assert.equal(fields[4].fullOnly, true, "static never patches");
    assert.equal(fields[5].fullOnly, true, "transient never patches");

    const callbacks = Callbacks.get(room);
    const changes = [];
    const titleChanges = [];
    callbacks.listen(room.state, "title", (current, previous) => titleChanges.push([current, previous]), false);
    callbacks.onChange("players", (id) => changes.push(id));

    const player = room.schema;
    const entityBody = concat([
        u32(1),
        u32(5),
        str("Neo"),
        u16(100),
        optionalF64(12.5),
        f32Array([0.5, -1.5]),
        u32(7),
        u8(1)
    ]);
    room._applySchema(worldFullSection(room._rootScalars.hash, [str("Arena")], [
        { id: 0, hash: player.hash, section: section(1, player.hash, entityBody) }
    ]));

    const neo = room.state.get(5);
    assert.equal(neo.name, "Neo");
    assert.equal(neo.hp, 100);
    assert.equal(neo.score, 12.5);
    assert.deepEqual(neo.aim, [0.5, -1.5]);
    assert.equal(neo.skin, 7);
    assert.equal(neo.blink, 1);
    assert.equal(room.state.title, "Arena");
    assert.deepEqual(titleChanges, [["Arena", ""]], "root string scalars patch like fields");

    changes.length = 0;
    room._applySchema(worldPatchSection(room._rootScalars.hash, 1, 0b1, [str("Dome")], [
        {
            id: 0,
            hash: player.hash,
            section: section(2, player.hash, concat([
                u32(1),
                u8(3),
                u32(5),
                u32((1 << 0) | (1 << 2) | (1 << 3)),
                str("Trinity"),
                u8(0),
                f32Array([2.5])
            ]))
        }
    ]));

    assert.equal(room.state.title, "Dome");
    assert.equal(neo.name, "Trinity");
    assert.equal(neo.score, null, "absent optional reads as null");
    assert.deepEqual(neo.aim, [2.5]);
    assert.equal(neo.hp, 100, "unmasked fields keep previous values");
    assert.deepEqual(changes, [5]);

    // A full frame with value-equal arrays must not fire field callbacks.
    const fieldChanges = [];
    const callbacks2 = Callbacks.get(room);
    callbacks2.listen(neo, "aim", (current, previous) => fieldChanges.push([current, previous]), false);
    room._applySchema(worldFullSection(room._rootScalars.hash, [str("Dome")], [
        {
            id: 0,
            hash: player.hash,
            section: section(1, player.hash, concat([
                u32(1),
                u32(5),
                str("Trinity"),
                u16(100),
                optionalF64(null),
                f32Array([2.5]),
                u32(7),
                u8(1)
            ]))
        }
    ]));
    assert.deepEqual(fieldChanges, [], "unchanged array contents are ignored");
}

// Static and transient flags survive a manual SchemaDef round-trip.
{
    const schema = new SchemaDef("Player", 1)
        .string("name")
        .f64("score").optional()
        .array("aim", "f32")
        .u32("skin").staticField()
        .u8("blink").transient();
    assert.equal(schema.fields[1].optional, true);
    assert.equal(schema.fields[2].element, "f32");
    assert.equal(schema.fields[3].fullOnly, true);
    assert.equal(schema.fields[4].fullOnly, true);
}

console.log("schema tests passed");