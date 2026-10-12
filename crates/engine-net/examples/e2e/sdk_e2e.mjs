// E2E: real @colyseus/sdk — WASD multi-player flow.
// Join -> own player spawns -> send input D -> verify server moves us ->
// verify patch stream keeps flowing. Run: node sdk_e2e.mjs <port>
import { Client } from '/mnt/SSD/Devloper/pannel/colyseus/packages/sdk/build/index.mjs';
import { t, schema } from '/mnt/SSD/Devloper/pannel/colyseus/node_modules/@colyseus/schema/build/index.mjs';

const Player = schema({ x: t.number(), y: t.number() }, 'Player');
const GameState = schema({ players: t.map(Player), tick: t.number() }, 'GameState');

const port = process.argv[2] ?? '7777';
const client = new Client(`ws://127.0.0.1:${port}`);

const timeout = setTimeout(() => {
    console.log('RESULT: TIMEOUT');
    process.exit(1);
}, 10000);

try {
    const room = await client.joinOrCreate('game_room', {}, GameState);
    console.log('[sdk] JOINED sessionId =', room.sessionId);

    let sawSelf = false;
    let xStart = null;
    let moved = false;
    let updates = 0;

    room.onStateChange((state) => {
        updates++;
        const me = state.players.get(room.sessionId);
        if (me && !sawSelf) {
            sawSelf = true;
            xStart = me.x;
            console.log(`[sdk] own player spawned at (${me.x.toFixed(1)}, ${me.y.toFixed(1)})`);
            console.log('[sdk] holding D (right)…');
        }
        if (sawSelf && me && xStart !== null) {
            if (me.x > xStart + 20) moved = true;
        }
        if (sawSelf && moved && updates > 10) {
            const me = state.players.get(room.sessionId);
            console.log(`[sdk] MOVED! x ${xStart.toFixed(1)} -> ${me.x.toFixed(1)}, players=${state.players.size}, tick=${state.tick}`);
            clearTimeout(timeout);
            console.log('RESULT: SUCCESS');
            process.exit(0);
        }
    });

    // Hold D every 33ms like the website does.
    const hold = setInterval(() => {
        if (!sawSelf) return;
        room.sendBytes('input', new Uint8Array([8])); // FLAG_D
    }, 33);
    setTimeout(() => clearInterval(hold), 8000);
} catch (err) {
    console.log('[sdk] ERROR:', err?.message ?? err);
    clearTimeout(timeout);
    console.log('RESULT: FAILED');
    process.exit(1);
}
