import { TestRoom } from '../sdk/testing/src/index.js';

const $ = id => document.getElementById(id);
let room = null;
let client = null;

function render() {
    if (!room) return;
    const players = {};
    for (const [id, p] of room.state.players) players[id] = { ...p };
    $('out').textContent = JSON.stringify({
        room: room.name,
        tick: room.state.tick,
        latencyMs: room.latencyMs,
        loss: room.loss,
        seed: room.options.seed,
        players,
    }, null, 2);
}

$('start').onclick = async () => {
    room?.close();
    room = await TestRoom.start('arena', {
        tickRate: Number($('tickRate').value) || 30,
        latencyMs: Number($('latency').value) || 0,
        loss: Number($('loss').value) || 0,
        seed: Number($('seed').value) || 42,
        step(state, c, input, dt) {
            if (!input) return;
            c.state.x = (c.state.x || 0) + (input.moveX || 0) * dt;
        },
    });
    client = await room.connect();
    render();
};

$('connect').onclick = async () => {
    if (!room) return;
    client = await room.connect();
    render();
};

$('tick').onclick = () => {
    if (!room) return;
    room.tick(10);
    render();
};

$('send').onclick = () => {
    if (!client) return;
    client.sendInput({ moveX: Number($('moveX').value) || 0 });
    render();
};

$('reset').onclick = () => {
    room?.close();
    room = null;
    client = null;
    $('out').textContent = 'room not started';
};
