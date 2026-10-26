import assert from "node:assert/strict";
import { test } from "node:test";
import CYG from "../cyg.js";

const { Room } = CYG;

// ---------------------------------------------------------------------------
// Browser stand-ins. The SDK only touches the surface stubbed here: `new
// WebSocket(url)`, `readyState`, `send`, and the event handlers on the
// signaling socket; and the subset of RTCPeerConnection the SDP dance uses.
// ---------------------------------------------------------------------------

class FakeWebSocket {
    static OPEN = 1;
    static instances = [];
    constructor(url) {
        this.url = url;
        this.readyState = FakeWebSocket.OPEN;
        this.sent = [];
        this.onopen = null;
        this.onmessage = null;
        this.onclose = null;
        this.onerror = null;
        FakeWebSocket.instances.push(this);
    }
    send(data) {
        this.sent.push(data);
    }
    /** Test helper: one JSON frame from the hub. */
    receive(obj) {
        this.onmessage({ data: JSON.stringify(obj) });
    }
}

class FakeDataChannel {
    constructor(label) {
        this.label = label;
        this.binaryType = "";
        this.readyState = "connecting";
        this.sent = [];
        this.onopen = null;
        this.onmessage = null;
        this.onclose = null;
        this.onerror = null;
    }
    send(data) {
        this.sent.push(data);
    }
}

class FakePeerConnection {
    static instances = [];
    constructor(config) {
        this.config = config;
        this.channels = [];
        this.localDescription = null;
        this.remoteDescription = null;
        this.signalingState = "stable";
        this.iceCandidates = [];
        this.onicecandidate = null;
        this.onconnectionstatechange = null;
        FakePeerConnection.instances.push(this);
    }
    createDataChannel(label, opts) {
        const ch = new FakeDataChannel(label);
        ch.opts = opts;
        this.channels.push(ch);
        return ch;
    }
    async createOffer() {
        return { type: "offer", sdp: "OFFER-SDP" };
    }
    async setLocalDescription(desc) {
        this.localDescription = desc;
        this.signalingState = "have-local-offer";
    }
    async setRemoteDescription(desc) {
        this.remoteDescription = desc;
        this.signalingState = "stable";
    }
    async addIceCandidate(cand) {
        this.iceCandidates.push(cand);
    }
    close() { /* noop */ }
}

function makeRoom(extra = {}) {
    const client = { _httpBase: () => "http://127.0.0.1:8124" };
    return new Room("ws://127.0.0.1:7777", "game_room", {
        client,
        sessionId: "sess-1",
        reconnectionToken: "SEAT-TOKEN",
        ...extra,
    });
}

function stubBrowser() {
    FakeWebSocket.instances = [];
    FakePeerConnection.instances = [];
    globalThis.WebSocket = FakeWebSocket;
    globalThis.RTCPeerConnection = FakePeerConnection;
}

function unstubBrowser() {
    delete globalThis.WebSocket;
    delete globalThis.RTCPeerConnection;
}

// ---------------------------------------------------------------------------
// The wire contract: URL, the four hub frames, and the channel handshake.
// ---------------------------------------------------------------------------

test("signal url carries the seat token over the control origin", () => {
    stubBrowser();
    try {
        const room = makeRoom();
        room.connect();
        const ws = FakeWebSocket.instances[0];
        assert.equal(ws.url, "ws://127.0.0.1:8124/ws/signal?seat=SEAT-TOKEN");
    } finally {
        unstubBrowser();
    }
});

test("offer candidate and answer follow the hub protocol", async () => {
    stubBrowser();
    try {
        const room = makeRoom();
        const done = room.connect();
        const ws = FakeWebSocket.instances[0];

        // Hub hands out ICE config first; the offer goes out only after it.
        ws.receive({ type: "ice", config: { iceServers: [{ urls: "stun:test" }] } });
        await new Promise((r) => setImmediate(r));
        const pc = FakePeerConnection.instances[0];
        assert.equal(pc.config.iceServers[0].urls, "stun:test", "hub ice config wins");
        assert.deepEqual(
            pc.channels.map((c) => c.label),
            ["game-unreliable", "game-reliable"],
            "the two contract channels are created",
        );
        const unreliable = pc.channels[0];
        assert.equal(unreliable.opts.ordered, false);
        assert.equal(unreliable.opts.maxRetransmits, 0);

        const offer = ws.sent.map((s) => JSON.parse(s)).find((m) => m.type === "offer");
        assert.ok(offer, "offer sent");
        assert.equal(offer.sdp, "OFFER-SDP");
        assert.equal(room.serverPc.localDescription.sdp, "OFFER-SDP");

        // Hub answer + a trickle candidate are applied to the pc.
        ws.receive({ type: "answer", sdp: "ANSWER-SDP" });
        ws.receive({ type: "candidate", candidate: "cand-1", sdpMid: "0", sdpIndex: 0 });
        await new Promise((r) => setImmediate(r));
        assert.equal(pc.remoteDescription.sdp, "ANSWER-SDP");
        assert.equal(pc.iceCandidates[0].candidate, "cand-1");

        // The reliable channel opening IS the room joining: 'open' fires and
        // the connect() promise resolves with the room.
        const opened = new Promise((r) => room.on("open", r));
        pc.channels[1].onopen();
        await opened;
        await done;
        assert.equal(room.isOpen, true);
    } finally {
        unstubBrowser();
    }
});

test("hub error rejects the connect before any channel opens", async () => {
    stubBrowser();
    try {
        const room = makeRoom();
        const done = room.connect();
        const ws = FakeWebSocket.instances[0];
        ws.receive({ type: "error", message: "data plane is not accepting signaling" });
        await assert.rejects(done, /not accepting signaling/);
        assert.equal(room.isOpen, false);
    } finally {
        unstubBrowser();
    }
});

test("without a seat the legacy game-port websocket is the fallback", async () => {
    stubBrowser();
    try {
        // No reconnectionToken/sessionId -> the rtc branch cannot build a
        // signal URL, so connect() must fall back to the game-port WS.
        const client = { _httpBase: () => "http://127.0.0.1:8124" };
        const room = new Room("ws://127.0.0.1:7777", "game_room", { client });
        const done = room.connect();
        const ws = FakeWebSocket.instances[0];
        assert.match(ws.url, /:7777\/game_room/, "legacy game-port WS is the fallback");
        ws.onopen();
        await done;
    } finally {
        unstubBrowser();
    }
});

// ---------------------------------------------------------------------------
// Colyseus API-parity lock: the exact surface the official SDK's example uses
// (`Callbacks.get(room)`, `room.onMessage('*')`, `room.onError/onLeave`,
// `room.sendUnreliable`, `room.leave(consented)`).
// ---------------------------------------------------------------------------

test("colyseus parity surface works end to end on a room", async () => {
    stubBrowser();
    try {
        const room = makeRoom();
        room.connect();
        const ws = FakeWebSocket.instances[0];
        assert.equal(room.roomId, "game_room", "roomId defaults to the room name");

        ws.receive({ type: "ice", config: { iceServers: [] } });
        await new Promise((r) => setImmediate(r));
        const pc = FakePeerConnection.instances[0];
        pc.channels[1].readyState = 'open';
        pc.channels[0].readyState = 'open';
        pc.channels[1].onopen();
        pc.channels[0].onopen();
        await new Promise((r) => setImmediate(r));

        // onMessage('*') sees every typed message as (type, payload).
        const seen = [];
        room.onMessage('*', (type, payload) => seen.push([type, payload]));
        let payload = null;
        room.onMessage('hello', (p) => { payload = p; });
        pc.channels[1].onmessage({
            data: (() => { const b = new Uint8Array(2 + 5 + 2); b[0] = 0x05; b[1] = 5;
                b.set(new TextEncoder().encode('hello'), 2); b.set([1, 2], 7); return b.buffer; })()
        });
        assert.deepEqual(payload, new Uint8Array([1, 2]));
        assert.equal(seen.length, 1);
        assert.equal(seen[0][0], 'hello');

        // send() rides the reliable DataChannel; sendUnreliable the other one.
        assert.equal(room.sendBytes('input', new Uint8Array([7])), true);
        assert.equal(pc.channels[1].sent.length, 1, "send goes over the reliable lane");
        assert.equal(room.sendUnreliable('pos', new Uint8Array([1])), true);
        assert.equal(pc.channels[0].sent.length, 1, "sendUnreliable rides the delta lane");

        // leave(consented) surfaces as onLeave(1000).
        const leftCode = new Promise((r) => room.onLeave((code) => r(code)));
        await room.leave();
        pc.channels[1].onclose();
        room.emit('close', { code: 1000 });
        assert.equal(await leftCode, 1000, "consented leave reports 1000");

        room.removeAllListeners();
    } finally {
        unstubBrowser();
    }
});

// ---------------------------------------------------------------------------
// Entity-proxy + root-scalar listen parity (`player.listen("x", cb)` directly
// on the entity, `callbacks.listen("coinX", cb)` for root scalars).
// ---------------------------------------------------------------------------

test("entities carry listen/onChange/bindTo and root scalars listen by name", async () => {
    stubBrowser();
    try {
        const room = makeRoom();
        room.connect();
        const ws = FakeWebSocket.instances[0];
        ws.receive({ type: "ice", config: { iceServers: [] } });
        await new Promise((r) => setImmediate(r));
        const pc = FakePeerConnection.instances[0];
        pc.channels[1].readyState = 'open';
        pc.channels[0].readyState = 'open';
        pc.channels[1].onopen();
        pc.channels[0].onopen();
        await new Promise((r) => setImmediate(r));

        // Root-scalar registry: the state entity is tracked once root scalars
        // exist (the descriptor attach path does this on the live server).
        room._rootScalars = { fields: [{ name: 'coinX' }, { name: 'coinY' }] };
        room._rootValues = { coinX: 400, coinY: 250 };
        room._trackEntity(room.state);

        // 1. callbacks.listen("coinX", cb) — root scalar by name.
        const rootSeen = [];
        const unroot = CYG.getStateCallbacks(room).listen('coinX', (v) => rootSeen.push(v));
        room._fireFieldChange(room.state, 'coinX', 512, 400);
        room._fireFieldChange(room.state, 'coinX', 600, 512);
        assert.deepEqual(rootSeen, [512, 600]);
        unroot();
        room._fireFieldChange(room.state, 'coinX', 700, 512);
        assert.deepEqual(rootSeen, [512, 600], "unsubscribe detaches");

        // 2. player.listen("x", cb) directly on the entity — the proxy form.
        const player = { id: 1, x: 10, y: 20 };
        room._trackEntity(player);
        const moves = [];
        const unlisten = player.listen('x', (v, prev) => moves.push([v, prev]));
        assert.deepEqual(moves, [[10, undefined]], "immediate default: current value fires now");
        room._fireFieldChange(player, 'x', 30, 10);
        assert.deepEqual(moves, [[10, undefined], [30, 10]]);

        // 3. player.onChange(cb) — instance-level any-change. A real patch
        //    fires the field listeners AND the any-change fan-out together
        //    (_fireFieldChange + _fireUpdate), mirroring the apply path.
        let anyCount = 0;
        const unany = player.onChange(() => { anyCount++; });
        room._fireFieldChange(player, 'y', 40, 20);
        room._fireUpdate(room.state, player, 1);
        assert.equal(anyCount, 1);
        unlisten();
        unany();
        room._fireFieldChange(player, 'x', 50, 30);
        room._fireUpdate(room.state, player, 1);
        assert.equal(anyCount, 1, "detached handlers stay silent");

        // 4. The proxy methods are invisible to serialization/enumeration.
        assert.deepEqual(Object.keys(player).sort(), ['id', 'x', 'y']);
    } finally {
        unstubBrowser();
    }
});
