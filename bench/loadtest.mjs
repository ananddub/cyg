#!/usr/bin/env node
/**
 * CYG load test.
 *
 * Spawns N schema clients against a running game server, drives inputs at a
 * fixed rate, and reports join latency, schema latency, update rate, and
 * counted traffic. It uses the public web SDK, not internal server hooks, so
 * the numbers include the real client-side protocol work.
 *
 *   node bench/loadtest.mjs --url http://127.0.0.1:7777 --room bench \
 *       --clients 16 --seconds 10 --hz 30 --move
 *
 * The schema fields must match the server's `SchemaDef` exactly:
 *
 *   node bench/loadtest.mjs --fields x:f64,y:f64,flags:u32
 *
 * Exit code is 0 only when every requested client connected, decoded a full
 * snapshot, and reported no schema/transport errors.
 */

import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const CYG = require(path.resolve(here, '..', 'sdk', 'client', 'web', 'cyg.js'));

const TYPES = new Set(['u8', 'u16', 'u32', 'i32', 'f32', 'f64', 'bool', 'i64']);
const DEFAULTS = {
    url: 'http://127.0.0.1:7777',
    room: 'bench',
    clients: 16,
    seconds: 10,
    hz: 30,
    move: false,
    webrtc: false,
    timeoutMs: 15000,
    schemaName: 'Player',
    schemaVersion: 1,
    fields: 'x:f64,y:f64,flags:u32',
    descriptor: null,
    script: null,
    ramp: 0,
    rooms: 1,
    roomId: null,
    reestablishAllDelay: 0,
    chaosSeconds: null,
    failP95Join: null,
    failP95Snapshot: null,
    json: false,
    verbose: false,
};

function usage() {
    console.log(`CYG load test

Usage:
  node bench/loadtest.mjs [options]

Options:
  --url <url>              Matchmake endpoint (default ${DEFAULTS.url})
  --room <name>            Room name to join or create (default ${DEFAULTS.room})
  --room-id <id>           Join an existing room by id instead of --room
  --clients <n>            Number of clients (default ${DEFAULTS.clients})
  --seconds <n>            Measurement window after all clients are ready (default ${DEFAULTS.seconds})
  --hz <n>                 Input messages per second per client (default ${DEFAULTS.hz})
  --move                   Send alternating right/left movement flags
  --webrtc                 Let the SDK negotiate WebRTC data channels (off by default)
  --descriptor <path>      Read the server's cyg.schema.bin (preferred)
  --fields <spec>          Manual schema fields, e.g. x:f64,y:f64,flags:u32 (default ${DEFAULTS.fields})
  --schema-name <name>     Manual schema name (default ${DEFAULTS.schemaName})
  --schema-version <n>     Manual schema version (default ${DEFAULTS.schemaVersion})
  --script <path>          Colyseus-style bot scenario module (onJoin/onTick/onMessage/onLeave/input)
  --ramp <n>               Max joins per second during the join phase (0 = sequential, no pacing)
  --rooms <n>              Spread clients across "room-0" .. "room-(n-1)" instead of one room
  --reestablish-all <ms>   Chaos phase: every <ms>, drop every connection and re-join all clients
                           (Colyseus @colyseus/loadtest --reestablishAllDelay parity)
  --chaos-seconds <n>      Duration of the chaos phase (default: same as --seconds; Ctrl-C ends it early)
  --fail-p95-join <ms>     Exit 1 when join p95 exceeds this threshold
  --fail-p95-snapshot <ms> Exit 1 when first-snapshot p95 exceeds this threshold
  --timeout <ms>           Per-client schema timeout (default ${DEFAULTS.timeoutMs})
  --json                   Print a machine-readable report
  --verbose                Print one row per client
  --help                   Show this help

The client joins sequentially so a low matchmake rate limit does not turn the
run into a retry contest. The measurement window starts only after every client
has decoded its first full snapshot.

Scenario scripts (Colyseus @colyseus/loadtest style) are ES modules exporting
any subset of these hooks; without a script the built-in join/input/sample
pattern runs unchanged:

  export async function onJoin(ctx) {}       // after the room is ready
  export function input(ctx) {}              // replaces the default 'input' frame;
                                             // return a Uint8Array, or null to skip
  export function onTick(ctx, elapsedMs) {}  // called at --hz on the input timer
  export function onMessage(ctx, type, payload) {}  // wildcard room messages
  export function onLeave(ctx, code) {}      // room left / socket closed

ctx = { index, roomName, room, client, state, now(), send(type, data),
        sendInput(bytes), request(type, data), rand(), config }.
See bench/scenarios/example.mjs for a working template.
`);
}

function parseArgs(argv) {
    const args = { ...DEFAULTS };
    const next = (flag) => {
        const value = argv[++i];
        if (value === undefined || value.startsWith('--')) {
            throw new Error(`${flag} needs a value`);
        }
        return value;
    };
    let i = 0;
    while (i < argv.length) {
        const arg = argv[i];
        switch (arg) {
            case '--url': args.url = next(arg); break;
            case '--room': args.room = next(arg); break;
            case '--room-id': args.roomId = next(arg); break;
            case '--clients': args.clients = Number(next(arg)); break;
            case '--seconds': args.seconds = Number(next(arg)); break;
            case '--hz': args.hz = Number(next(arg)); break;
            case '--move': args.move = true; break;
            case '--webrtc': args.webrtc = true; break;
            case '--descriptor': args.descriptor = next(arg); break;
            case '--fields': args.fields = next(arg); break;
            case '--schema-name': args.schemaName = next(arg); break;
            case '--schema-version': args.schemaVersion = Number(next(arg)); break;
            case '--timeout': args.timeoutMs = Number(next(arg)); break;
            case '--script': args.script = next(arg); break;
            case '--ramp': args.ramp = Number(next(arg)); break;
            case '--rooms': args.rooms = Number(next(arg)); break;
            case '--reestablish-all': args.reestablishAllDelay = Number(next(arg)); break;
            case '--chaos-seconds': args.chaosSeconds = Number(next(arg)); break;
            case '--fail-p95-join': args.failP95Join = Number(next(arg)); break;
            case '--fail-p95-snapshot': args.failP95Snapshot = Number(next(arg)); break;
            case '--json': args.json = true; break;
            case '--verbose': args.verbose = true; break;
            case '--help': case '-h': usage(); process.exit(0); break;
            default: throw new Error(`unknown option ${arg}`);
        }
        i++;
    }
    for (const [name, min] of [['clients', 1], ['seconds', 1], ['hz', 1], ['timeoutMs', 1], ['rooms', 1]]) {
        if (!Number.isFinite(args[name]) || args[name] < min) {
            throw new Error(`--${name} must be a number >= ${min}`);
        }
    }
    if (!Number.isFinite(args.ramp) || args.ramp < 0) {
        throw new Error('--ramp must be a number >= 0 (0 = no pacing)');
    }
    if (args.failP95Join !== null && (!Number.isFinite(args.failP95Join) || args.failP95Join <= 0)) {
        throw new Error('--fail-p95-join must be a number > 0');
    }
    if (args.failP95Snapshot !== null && (!Number.isFinite(args.failP95Snapshot) || args.failP95Snapshot <= 0)) {
        throw new Error('--fail-p95-snapshot must be a number > 0');
    }
    if (!Number.isFinite(args.reestablishAllDelay) || args.reestablishAllDelay < 0) {
        throw new Error('--reestablish-all must be a number >= 0 (ms, 0 = disabled)');
    }
    if (args.chaosSeconds !== null && (!Number.isFinite(args.chaosSeconds) || args.chaosSeconds < 1)) {
        throw new Error('--chaos-seconds must be a number >= 1');
    }
    if (args.reestablishAllDelay > 0 && args.chaosSeconds === null) {
        args.chaosSeconds = args.seconds;
    }
    if (args.roomId && args.rooms > 1) {
        throw new Error('--rooms cannot be combined with --room-id (a room id points at one room)');
    }
    if (!args.roomId && (args.room === undefined || args.room === null)) {
        throw new Error('either --room or --room-id is required');
    }
    if (!Number.isInteger(args.schemaVersion)) {
        throw new Error('--schema-version must be an integer');
    }
    return args;
}

function parseFields(spec) {
    const fields = spec.split(',').map((part) => {
        const [name, type] = part.trim().split(':');
        if (!name || !type) {
            throw new Error(`invalid field "${part}" (expected name:type)`);
        }
        if (!TYPES.has(type)) {
            throw new Error(`unknown field type "${type}" (expected one of ${[...TYPES].join(', ')})`);
        }
        return { name, type };
    });
    if (fields.length === 0) {
        throw new Error('--fields is empty');
    }
    return fields;
}

function buildSchema(config, fields) {
    const schema = new CYG.SchemaDef(config.schemaName, config.schemaVersion);
    for (const field of fields) {
        schema[field.type](field.name);
    }
    return schema;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function loadScenario(spec) {
    const resolved = path.resolve(spec);
    if (!fs.existsSync(resolved)) {
        throw new Error(`--script file not found: ${resolved}`);
    }
    let scenario;
    try {
        scenario = await import(pathToFileURL(resolved).href);
    } catch (error) {
        throw new Error(`failed to load --script ${resolved}: ${error.message}`);
    }
    const hooks = ['onJoin', 'input', 'onTick', 'onMessage', 'onLeave'];
    for (const hook of hooks) {
        const value = scenario[hook];
        if (value !== undefined && typeof value !== 'function') {
            throw new Error(`--script export "${hook}" must be a function`);
        }
    }
    if (scenario.options !== undefined && (typeof scenario.options !== 'object' || scenario.options === null)) {
        throw new Error('--script export "options" must be an object');
    }
    return scenario;
}

function makeContext(record, scenario, config, client) {
    const room = record.room;
    const ctx = {
        index: record.index,
        roomName: record.roomName,
        room,
        client,
        state: room.state,
        playerId: room.playerId,
        sessionId: room.sessionId,
        config,
        scenario,
        errors: record.errors,
        now: () => performance.now(),
        rand: Math.random,
        send: (type, data) => room.send(type, data),
        sendInput: (bytes) => {
            if (room.send('input', bytes)) record.inputsSent++;
        },
        request: (type, data) => room.request(type, data),
    };
    return ctx;
}

function safeHook(record, label, fn, ...args) {
    try {
        return fn(...args);
    } catch (error) {
        record.errors.push(`scenario ${label}: ${(error && error.message) || error}`);
        return undefined;
    }
}

function percentile(values, p) {
    if (values.length === 0) return 0;
    const sorted = [...values].sort((a, b) => a - b);
    const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
    return sorted[index];
}

function mean(values) {
    return values.length === 0 ? 0 : values.reduce((sum, value) => sum + value, 0) / values.length;
}

function ms(value) {
    return `${value.toFixed(1)} ms`;
}

function bytes(value) {
    if (value < 1024) return `${value} B`;
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
    return `${(value / 1024 / 1024).toFixed(2)} MiB`;
}

function waitForFullSnapshot(room, timeoutMs) {
    return new Promise((resolve, reject) => {
        let settled = false;
        const finish = (error) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            room.off('schema', onSchema);
            room.off('schemaerror', onSchemaError);
            room.off('close', onClose);
            if (error) reject(error);
            else resolve();
        };
        const onSchema = (event) => {
            if (event && (event.kind === 1 || event.kind === 3)) finish();
        };
        const onSchemaError = (message) => finish(new Error(String(message)));
        const onClose = () => finish(new Error('socket closed before the first full snapshot'));
        const timer = setTimeout(
            () => finish(new Error(`no full schema snapshot within ${timeoutMs} ms`)),
            timeoutMs
        );
        room.on('schema', onSchema);
        room.on('schemaerror', onSchemaError);
        room.on('close', onClose);
    });
}

function localEntity(record) {
    const { room } = record;
    const id = room.playerId != null ? room.playerId : Number(room.sessionId);
    return id != null && Number.isFinite(id) ? room.state.get(id) : undefined;
}

function attachCounters(record) {
    const { room } = record;
    room.on('schema', (event) => {
        if (!event) return;
        if (event.kind === 1 || event.kind === 3) record.fullFrames++;
        else if (event.kind === 2 || event.kind === 4) record.patchFrames++;
    });
    room.onStateChange(() => {
        record.stateChanges++;
        if (!record.tracking) return;
        const entity = localEntity(record);
        if (entity && typeof entity.x === 'number') {
            record.sawX = true;
            record.minX = Math.min(record.minX, entity.x);
            record.maxX = Math.max(record.maxX, entity.x);
        }
    });
    room.on('schemaerror', (message) => record.errors.push(String(message)));
    room.on('error', (error) => record.errors.push(String((error && error.message) || error)));
    room.on('close', () => { record.closed = true; });
}

async function connectClient(client, config, schema, descriptor, index, scenario) {
    const roomName = config.roomId
        ? config.roomId
        : (config.rooms > 1 ? `${config.room}-${index % config.rooms}` : config.room);
    const record = {
        index,
        roomName,
        room: null,
        joinMs: 0,
        readyMs: 0,
        initialFulls: 0,
        fullFrames: 0,
        patchFrames: 0,
        stateChanges: 0,
        inputsSent: 0,
        bytesIn: 0,
        bytesOut: 0,
        tracking: false,
        sawX: false,
        minX: Infinity,
        maxX: -Infinity,
        closed: false,
        errors: [],
        fatal: null,
    };

    const start = performance.now();
    try {
        record.room = config.roomId
            ? await client.joinById(config.roomId, { webrtc: config.webrtc, autoReconnect: false })
            : await client.joinOrCreate(roomName, {
                webrtc: config.webrtc,
                autoReconnect: false,
            });
        record.joinMs = performance.now() - start;
        attachCounters(record);
        if (descriptor) {
            // The host also pushes this descriptor on join; attach it locally
            // only if that frame has not arrived yet.
            if (!record.room.schema) record.room.useDescriptor(descriptor);
        } else {
            record.room.useSchema(schema);
        }
        await waitForFullSnapshot(record.room, config.timeoutMs);
        record.readyMs = performance.now() - start;
        record.initialFulls = record.fullFrames;
        if (scenario) {
            record.ctx = makeContext(record, scenario, config, client);
            if (scenario.onMessage) {
                record.room.onMessage('*', (type, payload) =>
                    safeHook(record, 'onMessage', scenario.onMessage, record.ctx, type, payload)
                );
            }
            record.room.on('leave', (code) => {
                if (scenario.onLeave) safeHook(record, 'onLeave', scenario.onLeave, record.ctx, code);
            });
            if (scenario.onJoin) await safeHook(record, 'onJoin', scenario.onJoin, record.ctx);
        }
    } catch (error) {
        record.fatal = error;
        record.errors.push(String((error && error.message) || error));
        if (record.room) {
            try { record.room.leave(); } catch (_) {}
        }
    }
    return record;
}

function startInput(record, config, scenario) {
    const period = 1000 / config.hz;
    const started = performance.now();
    record.timer = setInterval(() => {
        const { room } = record;
        if (!room || !room.isOpen) return;
        let payload = null;
        if (scenario && scenario.input) {
            const custom = safeHook(record, 'input', scenario.input, record.ctx);
            if (custom instanceof Uint8Array) payload = custom;
        } else {
            let flags = 0;
            if (config.move) {
                const elapsed = performance.now() - started;
                flags = Math.floor(elapsed / 1000) % 2 === 0 ? 8 : 4;
            }
            payload = new Uint8Array(4);
            new DataView(payload.buffer).setUint32(0, flags, true);
        }
        if (payload && room.send('input', payload)) record.inputsSent++;
        if (scenario && scenario.onTick) {
            const elapsed = performance.now() - started;
            const done = safeHook(record, 'onTick', scenario.onTick, record.ctx, elapsed);
            if (done instanceof Promise) {
                done.catch((error) =>
                    record.errors.push(`scenario onTick: ${(error && error.message) || error}`)
                );
            }
        }
    }, period);
}

function beginWindow(record) {
    const { room } = record;
    record.bytesInStart = room.bytesIn;
    record.bytesOutStart = room.bytesOut;
    record.fullFrames = 0;
    record.patchFrames = 0;
    record.stateChanges = 0;
    record.inputsSent = 0;
    record.sawX = false;
    record.minX = Infinity;
    record.maxX = -Infinity;
    record.tracking = true;
}

function endWindow(record) {
    record.tracking = false;
    if (record.timer) {
        clearInterval(record.timer);
        record.timer = null;
    }
    record.measure = {
        bytesIn: record.room.bytesIn - record.bytesInStart,
        bytesOut: record.room.bytesOut - record.bytesOutStart,
        fullFrames: record.fullFrames,
        patchFrames: record.patchFrames,
        stateChanges: record.stateChanges,
        inputsSent: record.inputsSent,
        updates: record.fullFrames + record.patchFrames,
        sawX: record.sawX,
        motionRange: record.sawX ? record.maxX - record.minX : 0,
        entities: record.room.state.size,
    };
}

function cleanup(records) {
    for (const record of records) {
        if (record.timer) clearInterval(record.timer);
        if (record.room) {
            try { record.room.leave(); } catch (_) {}
        }
    }
}

/**
 * Colyseus @colyseus/loadtest --reestablishAllDelay parity: repeatedly drop
 * every connection and re-join all clients, counting reconnect success and
 * latency. Runs until --chaos-seconds elapses or the run is interrupted.
 */
async function runChaos(client, config, schema, descriptor, scenario, isAborted) {
    const chaos = {
        intervalMs: config.reestablishAllDelay,
        seconds: config.chaosSeconds,
        cycles: 0,
        rejoined: 0,
        failed: 0,
        reconnectMs: [],
        errors: [],
    };
    const deadline = performance.now() + config.chaosSeconds * 1000;
    while (performance.now() < deadline && !isAborted()) {
        const alive = await abortedWait(
            Math.min(config.reestablishAllDelay, Math.max(0, deadline - performance.now())),
            isAborted
        );
        if (!alive) break;
        const fresh = [];
        let cycleOk = 0;
        for (let i = 0; i < config.clients && !isAborted(); i++) {
            const record = await connectClient(client, config, schema, descriptor, i, scenario);
            if (record.fatal) {
                chaos.failed++;
                chaos.errors.push(`rejoin ${i} failed: ${record.fatal.message || record.fatal}`);
            } else {
                cycleOk++;
                chaos.rejoined++;
                chaos.reconnectMs.push(record.readyMs);
            }
            fresh.push(record);
        }
        chaos.cycles++;
        if (!config.json) {
            process.stderr.write(`chaos cycle ${chaos.cycles}: ${cycleOk}/${config.clients} re-joined\r`);
        }
        cleanup(fresh);
    }
    if (!config.json) process.stderr.write('\n');
    return chaos;
}

function abortedWait(ms, isAborted) {
    return sleep(ms).then(() => !isAborted());
}

async function main() {
    const config = parseArgs(process.argv.slice(2));
    if (typeof WebSocket === 'undefined') {
        throw new Error('global WebSocket is missing — use Node 22+ or run with --experimental-websocket');
    }
    let descriptor = null;
    let fields = null;
    let schema = null;
    if (config.descriptor) {
        descriptor = fs.readFileSync(path.resolve(config.descriptor));
    } else {
        fields = parseFields(config.fields);
        schema = buildSchema(config, fields);
    }
    const scenario = config.script ? await loadScenario(config.script) : null;
    if (scenario && scenario.options) {
        for (const key of ['hz', 'seconds', 'move', 'webrtc']) {
            if (scenario.options[key] !== undefined && config[key] === DEFAULTS[key]) {
                config[key] = scenario.options[key];
            }
        }
    }

    const client = new CYG.Client(config.url);
    const records = [];
    let aborted = false;
    process.on('SIGINT', () => {
        aborted = true;
        console.error('\nloadtest: interrupted — reporting the completed window, if any');
    });

    try {
        const joinDelayMs = config.ramp > 0 ? 1000 / config.ramp : 0;
        for (let i = 0; i < config.clients && !aborted; i++) {
            const record = await connectClient(client, config, schema, descriptor, i, scenario);
            records.push(record);
            if (record.fatal) {
                throw new Error(`client ${i} failed: ${record.fatal.message || record.fatal}`);
            }
            if (!config.json) {
                process.stderr.write(`joined ${i + 1}/${config.clients}\r`);
            }
            if (joinDelayMs > 0 && i < config.clients - 1 && !aborted) {
                await sleep(joinDelayMs);
            }
        }
        if (!config.json) process.stderr.write('\n');

        if (records.length === 0) {
            throw new Error(aborted ? 'interrupted before any client joined' : 'no clients joined');
        }
        if (aborted) {
            throw new Error('interrupted before the measurement window');
        }

        for (const record of records) beginWindow(record);
        const windowStart = performance.now();
        for (const record of records) startInput(record, config, scenario);

        const deadline = windowStart + config.seconds * 1000;
        while (performance.now() < deadline && !aborted) {
            await sleep(Math.min(100, Math.max(0, deadline - performance.now())));
        }
        const durationSec = Math.max((performance.now() - windowStart) / 1000, 0.001);

        for (const record of records) endWindow(record);

        const measure = records.map((record) => record.measure);
        const joins = records.map((record) => record.joinMs);
        const readies = records.map((record) => record.readyMs);
        const totalBytesIn = measure.reduce((sum, item) => sum + item.bytesIn, 0);
        const totalBytesOut = measure.reduce((sum, item) => sum + item.bytesOut, 0);
        const totalUpdates = measure.reduce((sum, item) => sum + item.updates, 0);
        const totalFulls = measure.reduce((sum, item) => sum + item.fullFrames, 0);
        const totalPatches = measure.reduce((sum, item) => sum + item.patchFrames, 0);
        const totalInputs = measure.reduce((sum, item) => sum + item.inputsSent, 0);
        const entityCounts = measure.map((item) => item.entities);
        const moved = measure.filter((item) => item.sawX && item.motionRange > 1).length;
        const withX = measure.filter((item) => item.sawX).length;
        const errors = records.flatMap((record) => record.errors);
        const contract = records
            .map((record) => record.room && record.room.schema)
            .find((value) => value) || null;
        const reportFields = contract
            ? contract.fields.map((field) => `${field.name}:${field.type}`)
            : (fields || []).map((field) => `${field.name}:${field.type}`);
        const reportHash = contract
            ? `0x${(contract.hash >>> 0).toString(16).padStart(8, '0')}`
            : 'unknown';

        let chaos = null;
        if (config.reestablishAllDelay > 0 && !aborted) {
            // Drop the measured connections first so re-join starts clean.
            for (const record of records) {
                if (record.timer) clearInterval(record.timer);
                if (record.room) { try { record.room.leave(); } catch (_) {} }
            }
            chaos = await runChaos(client, config, schema, descriptor, scenario, () => aborted);
        }

        const joinP95 = percentile(joins, 95);
        const snapshotP95 = percentile(readies, 95);
        const gates = {
            p95JoinMs: config.failP95Join,
            p95JoinPass: config.failP95Join === null ? null : joinP95 <= config.failP95Join,
            p95SnapshotMs: config.failP95Snapshot,
            p95SnapshotPass: config.failP95Snapshot === null ? null : snapshotP95 <= config.failP95Snapshot,
        };

        const report = {
            endpoint: config.url,
            room: config.roomId ? null : config.room,
            roomId: config.roomId,
            rooms: config.rooms,
            script: config.script ? path.resolve(config.script) : null,
            ramp: config.ramp,
            schema: {
                name: contract ? contract.name : config.schemaName,
                version: contract ? contract.version : config.schemaVersion,
                hash: reportHash,
                source: descriptor ? path.resolve(config.descriptor) : 'manual --fields',
                fields: reportFields,
            },
            clients: { requested: config.clients, joined: records.length, ready: records.filter((record) => !record.fatal).length },
            window: {
                seconds: Number(durationSec.toFixed(3)),
                inputHz: config.hz,
                move: config.move,
                webrtc: config.webrtc,
                aborted,
            },
            joinMs: {
                p50: Number(percentile(joins, 50).toFixed(1)),
                p95: Number(joinP95.toFixed(1)),
                mean: Number(mean(joins).toFixed(1)),
            },
            firstSnapshotMs: {
                p50: Number(percentile(readies, 50).toFixed(1)),
                p95: Number(snapshotP95.toFixed(1)),
                mean: Number(mean(readies).toFixed(1)),
            },
            updates: {
                total: totalUpdates,
                full: totalFulls,
                patch: totalPatches,
                perClientPerSecond: Number((totalUpdates / records.length / durationSec).toFixed(1)),
            },
            traffic: {
                bytesIn: totalBytesIn,
                bytesOut: totalBytesOut,
                bytesInPerSecond: Math.round(totalBytesIn / durationSec),
                bytesOutPerSecond: Math.round(totalBytesOut / durationSec),
                bytesInPerClient: Math.round(totalBytesIn / records.length),
                bytesOutPerClient: Math.round(totalBytesOut / records.length),
            },
            inputs: { sent: totalInputs, perClientPerSecond: Number((totalInputs / records.length / durationSec).toFixed(1)) },
            state: {
                entitiesMin: entityCounts.length ? Math.min(...entityCounts) : 0,
                entitiesMax: entityCounts.length ? Math.max(...entityCounts) : 0,
                clientsWithMotion: moved,
                clientsReportingX: withX,
            },
            errors,
            gates,
            chaos: chaos
                ? {
                    intervalMs: chaos.intervalMs,
                    seconds: chaos.seconds,
                    cycles: chaos.cycles,
                    rejoined: chaos.rejoined,
                    failed: chaos.failed,
                    reconnectMs: {
                        p50: Number(percentile(chaos.reconnectMs, 50).toFixed(1)),
                        p95: Number(percentile(chaos.reconnectMs, 95).toFixed(1)),
                        mean: Number(mean(chaos.reconnectMs).toFixed(1)),
                    },
                    errors: chaos.errors,
                }
                : null,
            perClient: records.map((record) => ({
                index: record.index,
                roomName: record.roomName,
                joinMs: Number(record.joinMs.toFixed(1)),
                readyMs: Number(record.readyMs.toFixed(1)),
                initialFulls: record.initialFulls,
                ...record.measure,
                errors: record.errors,
            })),
        };

        if (config.json) {
            console.log(JSON.stringify(report, null, 2));
        } else {
            printReport(report, config);
        }
        const gatesFailed = Object.values(gates).some((value) => value === false);
        const chaosFailed = chaos !== null && chaos.failed > 0;
        process.exitCode =
            errors.length === 0 && records.length === config.clients && !gatesFailed && !chaosFailed
                ? 0
                : 1;
    } finally {
        cleanup(records);
    }
}

function printReport(report, config) {
    const row = (label, value) => console.log(`  ${label.padEnd(15)} ${value}`);
    console.log(`CYG load test — ${report.endpoint} (${report.roomId ? `room id "${report.roomId}"` : `room "${report.room}"${report.rooms > 1 ? ` × ${report.rooms} rooms` : ''}`}${report.script ? ` · script ${path.basename(report.script)}` : ''}${report.ramp > 0 ? ` · ramp ${report.ramp}/s` : ''}${report.chaos ? ` · chaos ${report.chaos.intervalMs}ms` : ''})`);
    row('schema', `${report.schema.name} v${report.schema.version} · ${report.schema.fields.join(', ')} · ${report.schema.hash}`);
    row('clients', `${report.clients.joined}/${report.clients.requested} joined · ${report.clients.ready} ready`);
    row('join', `p50 ${ms(report.joinMs.p50)} · p95 ${ms(report.joinMs.p95)} · mean ${ms(report.joinMs.mean)}`);
    row('first snapshot', `p50 ${ms(report.firstSnapshotMs.p50)} · p95 ${ms(report.firstSnapshotMs.p95)} · mean ${ms(report.firstSnapshotMs.mean)}`);
    row('window', `${report.window.seconds.toFixed(1)} s · inputs ${report.window.inputHz} Hz${config.move ? ' (alternating left/right)' : ''} · webrtc ${config.webrtc ? 'on' : 'off'}`);
    row('updates', `${report.updates.perClientPerSecond}/s per client · ${report.updates.total} frames (${report.updates.full} full, ${report.updates.patch} patch)`);
    row('traffic in', `${bytes(report.traffic.bytesIn)} total · ${bytes(report.traffic.bytesInPerSecond)}/s · ${bytes(report.traffic.bytesInPerClient)}/client`);
    row('traffic out', `${bytes(report.traffic.bytesOut)} total · ${bytes(report.traffic.bytesOutPerSecond)}/s · ${bytes(report.traffic.bytesOutPerClient)}/client`);
    row('inputs', `${report.inputs.sent} sent · ${report.inputs.perClientPerSecond}/s per client`);
    const motion = report.state.clientsReportingX > 0
        ? `${report.state.clientsWithMotion}/${report.state.clientsReportingX} clients with x moved`
        : 'x field not present';
    row('state', `${report.state.entitiesMin === report.state.entitiesMax ? report.state.entitiesMin : `${report.state.entitiesMin}–${report.state.entitiesMax}`} entities · ${motion}`);
    row('errors', report.errors.length === 0 ? '0' : `${report.errors.length} (${report.errors.slice(0, 3).join(' | ')})`);
    const gateParts = [];
    if (report.gates.p95JoinMs !== null) {
        gateParts.push(`join p95 ${report.joinMs.p95} <= ${report.gates.p95JoinMs}: ${report.gates.p95JoinPass ? 'pass' : 'FAIL'}`);
    }
    if (report.gates.p95SnapshotMs !== null) {
        gateParts.push(`snapshot p95 ${report.firstSnapshotMs.p95} <= ${report.gates.p95SnapshotMs}: ${report.gates.p95SnapshotPass ? 'pass' : 'FAIL'}`);
    }
    if (gateParts.length > 0) row('gates', gateParts.join(' · '));
    if (report.chaos) {
        row('chaos', `${report.chaos.cycles} cycles · ${report.chaos.rejoined} re-joined · ${report.chaos.failed} failed · reconnect p50 ${ms(report.chaos.reconnectMs.p50)} · p95 ${ms(report.chaos.reconnectMs.p95)}`);
    }

    if (config.verbose) {
        console.log('\n  client  join     ready    updates  in          out         entities  errors');
        for (const item of report.perClient) {
            console.log(
                `  ${String(item.index).padEnd(7)} ${String(item.joinMs).padEnd(8)} ${String(item.readyMs).padEnd(8)} ` +
                `${String(item.updates).padEnd(8)} ${bytes(item.bytesIn).padEnd(11)} ${bytes(item.bytesOut).padEnd(11)} ` +
                `${String(item.entities).padEnd(9)} ${item.errors.length}`
            );
        }
    }
}

main().then(
    () => process.exit(process.exitCode || 0),
    (error) => {
        console.error(`loadtest: ${error && error.message ? error.message : error}`);
        process.exit(2);
    }
);
