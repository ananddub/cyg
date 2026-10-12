// E2E: real @colyseus/sdk — WASD multi-player flow.
// Join -> own player spawns -> send input D -> verify server moves us ->
// verify patch stream keeps flowing. Run: node sdk_e2e.mjs <port>
import { Client } from '/mnt/SSD/Devloper/pannel/colyseus/packages/sdk/build/index.mjs';

const port = process.argv[2] ?? '7777';
const client = new Client(`ws://127.0.0.1:${port}`);

const timeout = setTimeout(() => {
    console.log('RESULT: TIMEOUT');
    process.exit(1);
}, 10000);

try {
    const room = await client.joinOrCreate('game_room');
    console.log('[sdk] JOINED sessionId =', room.sessionId);

    let sawSelf = false;
    let xStart = null;
    let moved = false;
    let updates = 0;

    room.onMessage("delta", (bytes) => {
        updates++;
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const myId = view.getBigInt64(0, true);
        let offset = 8;
        const count = view.getUint32(offset, true);
        offset += 4;

        let me = null;
        for (let i = 0; i < count; i++) {
            const id = view.getBigInt64(offset, true);
            offset += 8;
            const x = view.getFloat64(offset, true);
            offset += 8;
            const y = view.getFloat64(offset, true);
            offset += 8;
            const score = view.getFloat64(offset, true);
            offset += 8;
            const hp = view.getFloat64(offset, true);
            offset += 8;
            if (id === myId) {
                me = { id, x, y, score, hp };
            }
        }

        if (me && !sawSelf) {
            sawSelf = true;
            xStart = me.x;
            console.log(`[sdk] own player id=${myId} spawned at (${me.x.toFixed(1)}, ${me.y.toFixed(1)})`);
            console.log('[sdk] holding D (right)…');
        }
        if (sawSelf && me && xStart !== null) {
            if (me.x > xStart + 20) moved = true;
        }
        if (sawSelf && moved && updates > 10) {
            console.log(`[sdk] MOVED! x ${xStart.toFixed(1)} -> ${me.x.toFixed(1)}, players=${count}, updates=${updates}`);
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
