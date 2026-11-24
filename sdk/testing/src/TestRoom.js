import { FakeClock } from './FakeClock.js';
import { TestClient } from './TestClient.js';
import { mulberry32, seedFrom } from './rng.js';

/**
 * Deterministic authoritative loop for tests.
 *
 * `step(state, client, input, dt)` is the only game hook. Return nothing; mutate
 * `state` in place. `onJoin(state, client)` / `onLeave(state, client)` are
 * optional. State defaults to `{ tick: 0, players: new Map() }`.
 */
export class TestRoom {
    static async start(name, options = {}) {
        return new TestRoom(name, options);
    }

    constructor(name, options = {}) {
        this.name = name;
        this.options = options;
        this.clock = new FakeClock(options.tickRate || 30, options.startMs || 0);
        this.rng = mulberry32(options.seed ?? seedFrom(name));
        this.latencyMs = options.latencyMs || 0;
        this.loss = options.loss || 0;
        this.stepFn = options.step || (() => {});
        this.onJoinFn = options.onJoin || (() => {});
        this.onLeaveFn = options.onLeave || (() => {});
        this.onMessageFn = options.onMessage || (() => {});
        this.state = options.state || { tick: 0, players: new Map() };
        this.clients = new Map();
        this._nextId = 1;
        this._inbox = [];
        this._closed = false;
    }

    async connect(options = {}) {
        if (this._closed) throw new Error('room is closed');
        const id = this._nextId++;
        const client = new TestClient(this, id, options);
        const player = options.player || { id, x: 0, y: 0 };
        player.id = id;
        this.clients.set(id, client);
        this.state.players.set(id, player);
        client.state = player;
        this.onJoinFn(this.state, client, options);
        return client;
    }

    _queueInput(client, seq, input) {
        const due = this.clock.nowMs + this.latencyMs;
        if (this.loss > 0 && this.rng() < this.loss) return false;
        this._inbox.push({ kind: 'input', due, client, seq, input });
        return true;
    }

    _queueMessage(client, type, payload) {
        this._inbox.push({
            kind: 'message',
            due: this.clock.nowMs + this.latencyMs,
            client,
            type,
            payload,
        });
    }

    _leave(client) {
        this.clients.delete(client.id);
        this.state.players.delete(client.id);
        this.onLeaveFn(this.state, client);
    }

    /** Advance `count` fixed ticks and deliver everything due at each tick. */
    tick(count = 1) {
        if (this._closed) return this.state;
        const dt = 1 / this.clock.tickRate;
        for (let t = 0; t < count; t++) {
            this.clock.advanceTicks(1);
            this.state.tick = this.clock.tick;
            this._deliverDue();
            for (const client of this.clients.values()) {
                this.stepFn(this.state, client, client._lastInput || null, dt);
            }
        }
        return this.state;
    }

    /** Advance by wall milliseconds, running whole ticks. */
    advance(ms) {
        const ticks = Math.floor(ms / this.clock.intervalMs);
        if (ticks > 0) this.tick(ticks);
        return this.state;
    }

    _deliverDue() {
        const pending = [];
        const now = this.clock.nowMs;
        for (const item of this._inbox) {
            if (item.due <= now) {
                if (item.kind === 'input') {
                    item.client._lastInput = item.input;
                } else if (item.kind === 'message') {
                    this.onMessageFn(this.state, item.client, item.type, item.payload);
                    item.client.emit(item.type, item.payload);
                }
            } else {
                pending.push(item);
            }
        }
        this._inbox = pending;
    }

    close() {
        this._closed = true;
        for (const client of this.clients.values()) client.connected = false;
        this.clients.clear();
        this._inbox.length = 0;
    }
}
