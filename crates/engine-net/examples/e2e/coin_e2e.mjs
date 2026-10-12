// Coin Rush E2E: auto-play bot — steers toward the coin, verifies score.
import { Client } from '/mnt/SSD/Devloper/pannel/colyseus/packages/sdk/build/index.mjs';
import { t, schema } from '/mnt/SSD/Devloper/pannel/colyseus/node_modules/@colyseus/schema/build/index.mjs';

const Player = schema({ x: t.number(), y: t.number(), score: t.number() }, 'Player');
const GameState = schema({
    players: t.map(Player), tick: t.number(),
    coin_x: t.number(), coin_y: t.number(),
}, 'GameState');

const client = new Client('ws://127.0.0.1:7777');
const room = await client.joinOrCreate('game_room', {}, GameState);
console.log('[bot] joined');

let frames = 0;
room.onStateChange(state => {
    frames++;
    if (frames % 20 !== 1) return;
    const me = state.players.get(room.sessionId);
    if (!me) { console.log('[bot] no own player!'); return; }
    console.log(`[bot] score=${me.score} pos=(${me.x.toFixed(0)},${me.y.toFixed(0)}) coin=(${state.coin_x.toFixed(0)},${state.coin_y.toFixed(0)})`);

    if (me.score > 0) {
        console.log('RESULT: SUCCESS (coin collected, score = ' + me.score + ')');
        process.exit(0);
    }
    // steer toward coin
    const flags =
        (state.coin_y < me.y - 5 ? 1 : 0) |
        (state.coin_y > me.y + 5 ? 2 : 0) |
        (state.coin_x < me.x - 5 ? 4 : 0) |
        (state.coin_x > me.x + 5 ? 8 : 0);
    room.sendBytes('input', new Uint8Array([flags]));
});
setTimeout(() => { console.log('RESULT: TIMEOUT (no coin collected)'); process.exit(1); }, 9000);
