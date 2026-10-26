import assert from "node:assert/strict";
import { test } from "node:test";
import CYG from "../cyg.js";

const { Room, Storage } = CYG;

// fake transport: request frames capture karke reply bhejne ka control
function makeRoom() {
    const room = new Room("ws://localhost:19224", "sky3d", {
        client: { _httpBase: () => "http://127.0.0.1:19224" },
        sessionId: "s1",
        reconnectionToken: "T",
    });
    room.isOpen = true;
    room.signalWs = { readyState: 1, send() {} };
    room.serverChReliable = { readyState: "open", sent: [], send(f) { this.sent.push(f); } };
    return room;
}

test("request resolves on matching __res reply", async () => {
    const room = makeRoom();
    const p = room.request("login", { user: "b" });
    await new Promise(r => setTimeout(r, 10));
    const frame = room.serverChReliable.sent[0];
    assert.equal(frame[0], 0x05);
    // reply: [op][typeLen]['__res'][reqId u16][data] = 2 + 5 + 2 + 3 bytes
    const reply = new Uint8Array(2 + 5 + 2 + 3);
    reply[0] = 0x05; reply[1] = 5;
    reply[2] = 0x5F; reply[3] = 0x5F; reply[4] = 0x72; reply[5] = 0x65; reply[6] = 0x73; // __res
    reply[7] = 0; reply[8] = 1; // reqId = 1 (pehla request)
    reply[9] = 1; reply[10] = 2; reply[11] = 3;
    room._handleAppMessage(reply);
    const data = await p;
    assert.deepEqual(Array.from(data), [1, 2, 3]);
    assert.equal(room._pendingRequests.size, 0, "pending cleared");
});

test("request timeout rejects and cleans up", async () => {
    const room = makeRoom();
    await assert.rejects(room.request("slow", null, 50), /timed out/);
});

test("Storage wraps values with JSON roundtrip", () => {
    const calls = [];
    const fakeLS = {
        getItem: (k) => { calls.push("get:" + k); return null; },
        setItem: (k, v) => calls.push("set:" + k + "=" + v),
        removeItem: (k) => calls.push("rm:" + k),
    };
    const orig = globalThis.localStorage;
    globalThis.localStorage = fakeLS;
    try {
        Storage.set("score", 42);
        assert.ok(calls.some(c => c.startsWith("set:")), "set localStorage me gaya");
        assert.equal(Storage.get("score", 0), 0, "empty storage pe fallback");
    } finally {
        globalThis.localStorage = orig;
    }
});
