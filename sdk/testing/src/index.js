// Deterministic headless room harness.
//
// This is the contract a room test needs: a fake clock, a seeded RNG, a
// controllable network (latency + loss), ordered input delivery and a fixed
// step callback. It does not spawn a browser or a server, so the same test
// runs in CI in milliseconds.

export { TestRoom } from './TestRoom.js';
export { TestClient } from './TestClient.js';
export { FakeClock } from './FakeClock.js';
export { mulberry32, seedFrom } from './rng.js';
export { assertEventually, snapshot } from './assert.js';
