import assert from "node:assert/strict";
import { test } from "node:test";
import CYG from "../cyg.js";

const { Room, PredictDriver } = CYG;

function makeRoom() {
    const room = new Room("ws://localhost:19224", "sky3d", {
        client: { _httpBase: () => "http://127.0.0.1:19224" },
        sessionId: "s1",
        reconnectionToken: "T",
    });
    room.isOpen = true;
    return room;
}

// Mock guest instance implementing the documented predict contract. The
// scratch cyg_alloc hands out region 1024.. of page 0 — the driver stages
// input/server bytes there through the exported memory.
function mockInstance(log) {
    const memory = new WebAssembly.Memory({ initial: 1 });
    let scratch = 1024;
    let stateBytes = new Uint8Array([9, 9, 9, 9]);
    return {
        memory,
        log,
        cyg_alloc(len) {
            const ptr = scratch;
            scratch += len + 8;
            return ptr;
        },
        cyg_predict_tick(ptr, len, dtMs) { log.calls.push(["tick", ptr, len, dtMs]); },
        cyg_predicted_state_ptr() { return 2048; },
        cyg_predicted_state_len() { return stateBytes.length; },
        cyg_reconcile(sptr, slen, pptr, plen) {
            log.reconciles.push([sptr, slen, pptr, plen]);
            return 0.5;
        },
        cyg_drift_ema() { return 0.25; },
        cyg_drift_peak() { return 0.5; },
    };
}

test("predict driver parks without the full export set", () => {
    const driver = new PredictDriver(makeRoom(), { cyg_predict_tick() {} });
    assert.equal(driver.available, false);
    assert.equal(driver.tick(0), 0);
    assert.equal(driver.onAck(new Uint8Array(0)), undefined);
});

test("fixed-step accumulator runs the send budget and bounds the pending ring", () => {
    const log = { calls: [], reconciles: [] };
    const room = makeRoom();
    room.timeSync.tickRate = 30; // ~33.3ms steps
    const driver = new PredictDriver(room, mockInstance(log));
    assert.equal(driver.available, true);

    // The first tick establishes the origin (0 steps); 101ms later at 30Hz
    // → 3 steps (100ms is a hair under 3×1000/30 in float — by design the
    // accumulator never invents a step).
    assert.equal(driver.tick(0), 0, "first tick primes the accumulator");
    const n = driver.tick(101);
    assert.equal(n, 3);
    assert.equal(log.calls.length, 3);
    assert.equal(driver.pending.length, 3);

    // Ring bound: force 70 steps, max 64 pending survive.
    for (let i = 0; i < 70; i++) driver._step(33.3);
    assert.equal(driver.pending.length, 64);
});

test("latch outside the step loop, consume exactly once inside", () => {
    const log = { calls: [], reconciles: [] };
    const room = makeRoom();
    room.timeSync.tickRate = 30;
    const driver = new PredictDriver(room, mockInstance(log));

    // One tap on a multi-step frame: latched once, consumed on ONE step.
    driver.latch(3); // button 3 (arbitrary packed slot)
    driver.tick(0);
    driver.tick(101); // 3 steps
    assert.equal(log.calls.length, 3);

    const first = new Float64Array(driver.pending[0].input.buffer);
    const rest = new Float64Array(driver.pending[1].input.buffer);
    assert.equal(first[0], 3, "the edge landed on the first step");
    assert.equal(rest[0], 0, "and did not double-fire on later steps");
});

test("reconcile pairs the server delta with unacked inputs and mirrors drift", () => {
    const log = { calls: [], reconciles: [] };
    const room = makeRoom();
    room.timeSync.tickRate = 30;
    const driver = new PredictDriver(room, mockInstance(log));
    driver.tick(0);
    driver.tick(101);
    assert.equal(driver.pending.length, 3);

    // Server confirmed seq 2: seqs 1 and 2 are acked, 3 stays pending.
    room.ackedSeq = 2;
    driver.onAck(new Uint8Array([1, 2, 3, 4]));
    assert.equal(log.reconciles.length, 1);
    assert.equal(driver.pending.length, 1, "acked inputs trimmed");
    assert.equal(driver.driftEma, 0.25);
    assert.equal(driver.driftPeak, 0.5);
});

test("room.predict is a no-op stub until a guest instance is attached", () => {
    const room = makeRoom();
    assert.equal(room.predict.available, false, "feature-detect: no instance yet");
    assert.equal(room.predict.tick(0), 0);

    const log = { calls: [], reconciles: [] };
    assert.equal(room.attachPredict(mockInstance(log)), true, "full export set attaches");
    assert.equal(room.predict.available, true);
});

test("delta tail: strict 8-byte tail, RTT echo only when the stamp is known", () => {
    const room = makeRoom();

    // We "sent" a stamped input 100ms ago; the tail echoes its stamp.
    const stamp = 0xBEEF;
    room._stampTimes.set(stamp, performance.now() - 100);
    room._stampOrder.push(stamp);

    // Schema frame: [your_id i64]["OXS2"]...[acked u16][tick u32][stamp u16]
    const payload = new Uint8Array(8 + 9 + 8);
    payload[8] = 0x4F; payload[9] = 0x58; payload[10] = 0x53; payload[11] = 0x32; // OXS2
    const pv = new DataView(payload.buffer);
    pv.setUint16(payload.byteLength - 8, 42, true);
    pv.setUint32(payload.byteLength - 6, 777, true);
    pv.setUint16(payload.byteLength - 2, stamp, true);

    const t = room._parseDeltaTail(payload);
    assert.equal(t.len, 8, "always the 8-byte tail");
    assert.equal(t.acked, 42);
    assert.equal(t.tick, 777);
    assert.ok(room._echoRtt >= 90 && room._echoRtt <= 150, "RTT paired from the echo");

    const clock = room.clock;
    assert.ok(Math.abs(clock.smoothedRtt() - room._echoRtt) < 1e-6);
    assert.equal(clock.jitter(), room._echoJitter);
    assert.ok(Number.isFinite(clock.serverNow()));

    // Spectator / first frame: stamp 0 is valid, it just produces no RTT sample.
    pv.setUint16(payload.byteLength - 2, 0, true);
    const zero = room._parseDeltaTail(payload);
    assert.equal(zero.len, 8);
    assert.equal(zero.acked, 42);
    assert.equal(zero.tick, 777);

    // No 6-byte fallback exists. A frame shorter than the one contract tail
    // fails loudly instead of being guessed.
    assert.throws(() => room._parseDeltaTail(new Uint8Array(6)), /delta tail too short/);
});

test("sendSchema couples the network sequence and the predicted frame", () => {
    const log = { calls: [], reconciles: [] };
    const room = makeRoom();
    room.inputSeq = 41;
    room._packSchemaInput = (obj) => new Uint8Array([41, 0, 7, 8]); // [seq u16][fields]
    room._sendPackedInput = (body) => { log.sent = body.slice(); return true; };
    const driver = new PredictDriver(room, mockInstance(log));
    assert.equal(driver.sendSchema({ moveX: 1 }, 50), true);
    assert.deepEqual([...log.sent], [41, 0, 7, 8], "wire body keeps the seq");
    assert.equal(log.calls[0][2], 2, "predict reader starts after the seq");
    assert.equal(driver.pending.length, 1);
    assert.equal(driver.pending[0].seq, 41);
    assert.deepEqual([...driver.pending[0].input], [7, 8]);
});

test("readServerState converts an authoritative frame to the flat reconcile mirror", () => {
    const log = { calls: [], reconciles: [] };
    const room = makeRoom();
    const driver = new PredictDriver(room, mockInstance(log), {
        readServerState: (bytes) => new Float64Array([bytes[0], 4, 9]),
    });
    driver.onAck(new Uint8Array([3]));
    assert.equal(log.reconciles.length, 1);
    assert.equal(log.reconciles[0][1], 24, "three f64 values staged");
});
