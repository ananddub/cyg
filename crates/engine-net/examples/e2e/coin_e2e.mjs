// Coin Rush E2E: auto-play bot — steers toward the coin, verifies score.
import { Client } from '/mnt/SSD/Devloper/pannel/colyseus/packages/sdk/build/index.mjs';

const client = new Client('ws://127.0.0.1:7777');
const room = await client.joinOrCreate('game_room');
console.log('[bot] joined sessionId =', room.sessionId);

let frames = 0;
room.onMessage("delta", (bytes) => {
    frames++;
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
        if (id === myId) {
            me = { id, x, y, score };
        }
    }
    const coinX = view.getFloat64(offset, true);
    offset += 8;
    const coinY = view.getFloat64(offset, true);

    if (frames % 20 !== 1) return;
    if (!me) { console.log('[bot] waiting for player assignment...'); return; }
    console.log(`[bot] id=${myId} score=${me.score} pos=(${me.x.toFixed(0)},${me.y.toFixed(0)}) coin=(${coinX.toFixed(0)},${coinY.toFixed(0)})`);

    if (me.score > 0) {
        console.log('RESULT: SUCCESS (coin collected, score = ' + me.score + ')');
        process.exit(0);
    }

    // steer toward coin
    const flags =
        (coinY < me.y - 5 ? 1 : 0) |
        (coinY > me.y + 5 ? 2 : 0) |
        (coinX < me.x - 5 ? 4 : 0) |
        (coinX > me.x + 5 ? 8 : 0);
    room.sendBytes('input', new Uint8Array([flags]));
});

setTimeout(() => { console.log('RESULT: TIMEOUT (no coin collected)'); process.exit(1); }, 9000);
