# `@cyg/client`

Zero-dependency browser/Node client for CYG.

```js
const room = await new CYG.Client('http://localhost:7777')
    .joinOrCreate('game_room');
```

The UMD bundle is `cyg.js`; `cyg.d.ts` carries the TypeScript types.
It is also served by the game transport at `/cyg.js`, so a generated
project needs no bundler:

```html
<script src="/cyg.js"></script>
```

## What it provides

- **Matchmaking** — `joinOrCreate`, `join`, `create`, `joinById`, `spectate`,
  `reconnect`, `getAvailableRooms`, `onAuth`.
- **Typed messages** — `room.send(type, data)`, `sendJSON`, `sendBytes`,
  `sendRaw`, `onMessage`.
- **Schema state** — the host sends a binary descriptor on join; the SDK builds
  `room.schema`/`room.schemas` and attaches `room.state` (a Map) automatically.
  Register handlers with `getStateCallbacks(room)` (`onAdd`, `onRemove`, `onChange`,
  `listen`, `bindTo`) or the legacy `room.onAdd/onChange/onRemove/onStateChange`.
- **Reconnect** — exponential backoff with a retained seat token; terminal
  close codes are not retried.
- **Inspector** — `new CYG.Inspector(room).attach()` renders a floating
  development panel with the schema hash and decoded entities.
- **Transport** — WebSocket by default, with the SDK's existing WebRTC
  DataChannel, voice mesh, interpolation, replay, and rollback helpers still
  available for custom/legacy layouts. Pass `{ webrtc: false }` to
  `joinOrCreate`/`join` to keep a session on WebSocket only.
- **Metering** — `room.bytesIn` and `room.bytesOut` count transport bytes, so
  the bundled [`bench/loadtest.mjs`](../../bench/loadtest.mjs) can report
  bandwidth without a packet capture.

```html
<script src="/cyg.js"></script>
<script>
const room = await new CYG.Client('http://localhost:7777')
    .joinOrCreate('game_room');

// No schema file: the join handshake delivered the descriptor.
const callbacks = CYG.getStateCallbacks(room);
callbacks.onAdd('players', (player, id) => {
    callbacks.listen(player, 'x', (current, previous) => { /* move */ });
});
room.onStateChange((state) => render(state.players.values()));
room.on('schemaerror', console.error);
</script>
```

The schema field names, types, and order must match the server's `SchemaDef`
exactly; the SDK checks a hash and reports a mismatch instead of decoding
garbage. The descriptor is generated from the same `schema.ts` the server used,
so the lists cannot drift. For non-schema clients, a hand-written
`new CYG.SchemaDef(...)` can still be attached with `useSchema(...)`.

## Tests

```bash
npm test
```

Runs the byte-level schema decoder tests. They cover hash parity with the
AssemblyScript encoder, full snapshots, adds/removes/updates, patches that race
the baseline, and mismatched schemas.
