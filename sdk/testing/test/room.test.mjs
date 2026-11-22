import test from 'node:test';
import assert from 'node:assert/strict';
import { TestRoom, FakeClock, snapshot } from '../src/index.js';

test('a deterministic room advances movement at a fixed step', async () => {
    const room = await TestRoom.start('arena', {
        tickRate: 10,
        seed: 7,
        step(state, client, input, dt) {
            if (!input) return;
            client.state.x += (input.moveX || 0) * dt;
        },
    });
    const a = await room.connect();
    a.sendInput({ moveX: 10 });
    room.tick(5);
    assert.equal(room.state.players.get(a.id).x, 5);
    assert.equal(snapshot(room.state).tick, 5);
});

test('latency and loss are deterministic for a seed', async () => {
    const room = await TestRoom.start('net', { tickRate: 10, latencyMs: 100, loss: 0.5, seed: 3 });
    const a = await room.connect();
    const accepted = [];
    for (let i = 0; i < 20; i++) accepted.push(a.sendInput({ moveX: 1 }));
    assert.equal(accepted.filter(Boolean).length, room._inbox.length);
    assert.ok(accepted.includes(false), 'seed 3 must drop at least one frame');
});

test('fake clock is manual', () => {
    const clock = new FakeClock(20);
    clock.advanceTicks(4);
    assert.equal(clock.tick, 4);
    assert.equal(clock.nowMs, 200);
});
