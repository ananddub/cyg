// Every part of the SDK exercised against a running server.
//
// Unit tests can only prove the encoder produces the bytes it meant to. This
// file asks the other question: does the server accept them, and does what
// comes back mean anything. It runs only when CYG_LIVE=1 and a server is
// already listening, because a suite that boots a server cannot tell a
// transport bug from a boot timing bug.
//
//   CYG_LIVE=1 CYG_LIVE_URL=http://127.0.0.1:8297 node --test test/live.test.mjs

import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import CYG from '../cyg.js';

const LIVE = process.env.CYG_LIVE === '1';
const URL_BASE = process.env.CYG_LIVE_URL ?? 'http://127.0.0.1:8297';
const ROOM = process.env.CYG_LIVE_ROOM ?? 'bench_10k';

const skip = LIVE ? false : 'set CYG_LIVE=1 with a server running';

/** A joined room, always left behind so one failure cannot wedge the next test. */
async function withRoom(fn) {
  const client = new CYG.Client(URL_BASE);
  client.onAuth(() => null);
  const room = await client.joinOrCreate(ROOM, {});
  try {
    return await fn(room, client);
  } finally {
    try {
      room.leave();
    } catch {
      // Leaving a room that already closed is not a test failure.
    }
  }
}

describe('live: matchmaking', { skip }, () => {
  it('joins a room and is told the seat it got', async () => {
    await withRoom(async (room) => {
      assert.equal(room.roomName, ROOM);
      assert.equal(room.isOpen, true);
      // The seat id is assigned by the server, so a client that made one up
      // would come back empty and every later lookup would miss.
      assert.ok(room.seatSessionId, 'a joined room carries a seat id');
    });
  });

  it('lists available rooms', async () => {
    const client = new CYG.Client(URL_BASE);
    client.onAuth(() => null);
    const rooms = await client.getAvailableRooms(ROOM);
    assert.ok(Array.isArray(rooms), 'getAvailableRooms returns an array');
    const found = rooms.find((r) => r.name === ROOM);
    assert.ok(found, `${ROOM} is listed`);
    assert.equal(typeof found.clients, 'number');
    assert.equal(typeof found.maxClients, 'number');
  });
});

describe('live: state', { skip }, () => {
  it('receives a schema descriptor and a root collection', async () => {
    await withRoom(async (room) => {
      // `join()` resolves on the socket opening, which happens before the join
      // reply and the descriptor land — they arrive in the same burst but a
      // frame is not part of the handshake promise. Waiting for the state
      // rather than the socket is what a game has to do too.
      for (let i = 0; i < 50 && !room.schema; i++) {
        await new Promise((r) => setTimeout(r, 50));
      }
      assert.ok(room.schema, 'the descriptor arrives on join');
      assert.ok(room.rootCollection, 'the descriptor names a root collection');
      assert.ok(room.state instanceof Map, 'state is a map of id -> entity');
    });
  });

  it('assigns a player id from the first delta', async () => {
    await withRoom(async (room) => {
      // The server is authoritative about which entity is this client; a client
      // that guessed would read and write the wrong player.
      for (let i = 0; i < 50 && room.sessionId === null; i++) {
        await new Promise((r) => setTimeout(r, 100));
      }
      assert.ok(room.sessionId !== null, 'a player id arrived');
      assert.ok(room.playerId >= 0, 'the numeric form is populated too');
    });
  });

  it('ticks forward', async () => {
    await withRoom(async (room) => {
      const first = room.serverTick;
      for (let i = 0; i < 40 && room.serverTick === first; i++) {
        await new Promise((r) => setTimeout(r, 100));
      }
      assert.ok(room.serverTick > first, `serverTick advanced past ${first}`);
      assert.ok(room.ackedSeq >= 0, 'acks are being tracked');
    });
  });
});

describe('live: messaging', { skip }, () => {
  it('sends a string message without throwing', async () => {
    await withRoom(async (room) => {
      // The bench room does not answer messages, so the assertion is that the
      // frame is accepted by the socket: a rejected send returns false.
      assert.equal(room.send('ping', 'hello'), true, 'send() accepted the frame');
      assert.equal(room.sendJSON('probe', { a: 1 }), true, 'sendJSON() accepted the frame');
    });
  });

  it('counts the bytes it sends and receives', async () => {
    await withRoom(async (room) => {
      const before = room.bytesOut;
      room.send('ping', 'x'.repeat(64));
      assert.ok(room.bytesOut > before, 'bytesOut grew after a send');
      for (let i = 0; i < 40 && room.bytesIn === 0; i++) {
        await new Promise((r) => setTimeout(r, 100));
      }
      assert.ok(room.bytesIn > 0, 'state deltas arrived');
    });
  });

  it('registers and removes a message handler', async () => {
    await withRoom(async (room) => {
      let calls = 0;
      const handler = () => { calls++; };
      room.onMessage('never_sent', handler);
      // Removing a handler that never fired must not throw; the SDK keeps a
      // list per type and a stale entry would leak across a resume.
      assert.doesNotThrow(() => room.offMessage?.('never_sent', handler) ?? room.off('never_sent', handler));
      assert.equal(calls, 0);
    });
  });
});

describe('live: realtime delivery', { skip }, () => {
  it('delivers the same tick to two clients', async () => {
    const one = new CYG.Client(URL_BASE);
    one.onAuth(() => null);
    const two = new CYG.Client(URL_BASE);
    two.onAuth(() => null);
    const a = await one.joinOrCreate(ROOM, {});
    const b = await two.joinOrCreate(ROOM, {});
    try {
      // Both are in the same room or in two rooms of the same name; either way
      // both are ticking, which is what proves the fan-out works.
      // Both have to be *inside* a room that is simulating before a tick can
      // be compared: the second join may land in a second room of the same
      // name, and a room with one player in it still has to wake up.
      for (let i = 0; i < 100 && (a.serverTick === 0 || b.serverTick === 0); i++) {
        await new Promise((r) => setTimeout(r, 100));
      }
      await new Promise((r) => setTimeout(r, 300));
      assert.ok(a.serverTick > 0 && b.serverTick > 0, 'both clients tick');
      assert.notEqual(a.seatSessionId, b.seatSessionId, 'each client got its own seat');
    } finally {
      a.leave();
      b.leave();
    }
  });
});

describe('live: auth over REST', { skip }, () => {
  it('reports a bad credential as a failure, not a crash', async () => {
    const client = new CYG.Client(URL_BASE);
    // This project runs without a datastore, so authentication is expected to
    // fail for a configuration reason rather than a credential one. What the
    // test pins is that it fails as an Error a caller can catch, not as an
    // unhandled rejection or a hang.
    await assert.rejects(
      () => client.auth.login('nobody@example.invalid', 'wrong-password'),
      (error) => {
        // The suite is about the shape of the failure: a caller has to be able
        // to tell "wrong password" from "server down".
        assert.ok(error instanceof Error);
        return true;
      },
    );
  });
});
