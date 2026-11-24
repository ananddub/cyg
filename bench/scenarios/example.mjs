/**
 * Example CYG loadtest scenario (Colyseus @colyseus/loadtest style).
 *
 * Export any subset of these hooks; anything you leave out falls back to the
 * built-in behaviour (default 'input' frame at --hz, no message handlers).
 *
 *   node bench/loadtest.mjs --url http://127.0.0.1:7777 --room bench \
 *       --clients 50 --seconds 20 --script bench/scenarios/example.mjs
 *
 * ctx = { index, roomName, room, client, state, playerId, sessionId,
 *         config, errors, now(), rand(), send(type, data), sendInput(bytes),
 *         request(type, data) }.
 *
 * `options` can override defaults for flags the user did not pass explicitly:
 * { hz, seconds, move, webrtc }.
 */

export const options = {
    hz: 20,
    move: true,
};

// Called once per bot right after the room is ready (schema decoded).
export async function onJoin(ctx) {
    // Example: announce presence on the chat lane.
    ctx.send('chat', JSON.stringify({ hello: `bot-${ctx.index}` }));
}

// Replaces the default 4-byte 'input' frame. Return a Uint8Array, or null to
// skip this tick (e.g. bots that only act occasionally).
export function input(ctx) {
    const payload = new Uint8Array(4);
    new DataView(payload.buffer).setUint32(0, ctx.rand() < 0.5 ? 8 : 4, true);
    return payload;
}

// Called on the same timer as `input`, at --hz. Good for scripted behaviour
// beyond the input lane (requests, sporadic chat, state reads).
export function onTick(ctx, elapsedMs) {
    if (ctx.index === 0 && Math.floor(elapsedMs / 5000) !== Math.floor((elapsedMs - 33) / 5000)) {
        ctx.send('ping', 'tick');
    }
}

// Wildcard handler for every typed server message.
export function onMessage(ctx, type, payload) {
    // Add nothing by default — returning normally keeps error counters clean.
}

// Room left or socket closed; code is the WebSocket close code.
export function onLeave(ctx, code) {}
