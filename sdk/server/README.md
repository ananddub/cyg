# 🎮 CYG Multi-Language Room SDK Interfaces

CYG is language-agnostic. Game rooms compile into WebAssembly and execute inside a sandboxed Wasmtime instance with native ~17ns invocation speed.

Developers write authoritative multiplayer game logic using pure, high-level object-oriented or idiomatic room classes without needing to write low-level WebAssembly C-ABI functions.

## Three independent room channels

CYG keeps these separate so each one can be reasoned about on its own:

| Channel | Server API | Client API | Frequency |
| :--- | :--- | :--- | :--- |
| Local room events | `this.on(name, cb)` / `this.emit(name, id, a, b)` | — | In-process only |
| Typed messages | `onMessage(name, cb)` / `broadcast(name, bytes)` / `sendTo(id, name, bytes)` | `room.send(name, data)` / `room.onMessage(name, cb)` | On demand, reliable |
| Continuous state | `SchemaState.seal()` in `onTick(dt)` | `room.state` (auto-attached) + `getStateCallbacks(room)` | 20 / 30 / 60 / 120 Hz |

The browser SDK is CYG's own (`sdk/web/cyg.js`); it does not depend on Colyseus or msgpack. Typed messages use this envelope:

```text
[0x05][type_len u8][UTF-8 type][opaque payload]
```

`type_len` is 1..255 bytes. The host routes the frame to the guest, and the guest routes it to the matching `onMessage` handler. Outbound messages are queued by the guest (`host_send`) and written by the host loop after `onTick`, and immediately after `onJoin` / `onInput` replies, so guest code never touches a socket. The SDK injects the room instance as the last callback argument, which keeps callbacks usable with AssemblyScript's no-GC `stub` runtime (captured closures are not available there).

```typescript
// server: room.ts
this.on("discard", (id, a, b, room) => {
    // local only; call room.broadcast() if clients need to know
});

this.onMessage("chat", (id, reader, room) => {
    room.broadcastText("chat", "hello");
    room.sendTo(id, "ack", null);
});
```

```javascript
// browser: website.html
const client = new CYG.Client('http://127.0.0.1:7777');
const room = await client.joinOrCreate('game_room');

room.onMessage('chat', (payload) => {
    console.log(new TextDecoder().decode(payload));
});
room.send('chat', new Uint8Array([1, 2, 3]));
```

### Continuous state (schema)

Declare fields once in Colyseus v5 syntax in `schema.ts`; `ox-game run` /
`ox-game schema` generates `cyg.schema.ts` for the room and
`cyg.schema.bin`, a compact descriptor the host sends to every client on
join. The browser builds its schema from that descriptor, so the field lists
cannot drift and no client schema file is needed. The first frame is a full
snapshot; later frames carry only changed fields. A client that reconnects gets
the descriptor again and a fresh full snapshot.

```ts
// schema.ts
import { schema, t } from "@colyseus/schema";

export const Player = schema({
    x: t.number(),
    y: t.number(),
    flags: t.uint32(),
}, "Player");

export const State = schema({ players: t.map(Player) }, "State");
```

```typescript
// server: room.ts — generated class, no local SchemaDef
import { Room, SchemaState } from "./cyg-server";
import { Player, PlayerSchema } from "./cyg.schema";

export class GameRoom extends Room {
    state: SchemaState = new SchemaState(PlayerSchema);

    onJoin(id: i64, x: f64, y: f64): void {
        const p = new Player();
        p.x = x;
        p.y = y;
        this.state.set(id as u32, p);
    }
    onTick(dt: f64): i64 {
        return this.state.seal();
    }
}
```

```javascript
// browser — only /cyg.js is needed
const room = await new CYG.Client('http://127.0.0.1:7777')
    .joinOrCreate('game_room');

const callbacks = CYG.getStateCallbacks(room);
callbacks.onAdd('players', (player, id) => { /* ... */ });
callbacks.listen(player, 'x', (current, previous) => { /* ... */ });
callbacks.onRemove('players', (player, id) => { /* ... */ });
room.onStateChange((state) => render(state.players.values()));
```

The descriptor is the only schema source: the browser builds its declarations
from it on join, and `schema.ts` is the only file you edit.

### Per-session visibility

`.ownerOnly()` fields are owner-only by default. With `allow_views = true`, a room
can narrow a seat further without touching the wire format:

```ts
onJoin(client: Client) {
    this.views.onlySelf(client.id);                    // drop other rows
    this.views.hide(client.id, "players", "inventory"); // hide one field
    // this.views.hideRoot(client.id, "tick");         // hide a root scalar
}
```

Views only subtract. `.ownerOnly()` wins over every view, and a changed view makes
the host send that player a fresh full snapshot so stale values cannot linger.
`this.views.reset(client.id)` restores the default view.

### Lossy fields

Mark a field `.unreliable()` when losing an update is cheaper than delaying the
room. The host strips it from the reliable patch and sends it in a separate
tick-stamped frame on the lossy lane; the client applies newest-wins without
advancing the reliable tick.

```ts
export const Player = schema({
    x:  t.f32().range(-512, 512, 20).unreliable(),
    z:  t.f32().range(-512, 512, 20).unreliable(),
    hp: t.uint8(),   // must arrive
}, "Player");
```

### Plugins

```ts
class Arena extends Room {
    onCreate() {
        this.use(new MatchTimerPlugin(5));
    }
}
```

Plugins run in install order and are fed `onCreate`, `onJoin`, `onInput`,
`onTick`, `onLeave` and `onDispose`. AssemblyScript has no exception
handling, so a plugin that traps surfaces at the wasm boundary and the host
reports it; keep plugin hooks non-throwing and move fallible work into a
plain method the room calls explicitly.

### Snapshot priority and byte budget

Every generated entity inherits `priority`. Higher priority rows are encoded
first; when a byte budget is set, rows that do not fit stay dirty and ride the
next tick. Adds and removes are structural and always sent.

```ts
onCreate(max: i32): void {
    this.setState(this.state);
    this.setSnapshotBudget(1200); // bytes per patch; 0 = unlimited
}

// In your simulation, mark the row the camera needs most.
player.priority = 100;
remoteNoise.priority = 0;
```

For multi-collection worlds, `setSnapshotBudget` propagates to every
collection. The host's AOI/view filtering still runs after this, so a per-view
client can receive less than the global budget.

### Generic rewind

With `allow_rewind = true`, `this.rewind` stores an opaque `f64` vector per
entity, bounded to 256 samples and 32 fields. Physics lag compensation is one consumer; hit validation and
cooldowns use the same history.

```ts
const sample = new StaticArray<f64>(2);
sample[0] = player.x;
sample[1] = player.y;
this.rewind.recordAt(player.id, sample, clockMs());

// Time-based lag compensation using the shooter's own input timestamp.
// Future timestamps clamp to now; old ones clamp to the rewind window.
const frame = this.inputs.latest(shooterId);
const seen = new StaticArray<f64>(2);
if (frame) this.rewind.atClientMs(targetId, frame.clientTimeMs as f64, seen);

// Lower-level form: query an explicit room time.
this.rewind.atMs(targetId, clockMs() - 180.0, seen);
```

`recordNow`/`at` are tick-exact. `recordAt`/`atMs` store millisecond
timestamps and linearly interpolate between the surrounding samples (clamped
to the oldest/newest), which is what hit validation wants when the shooter's
timestamp falls between two simulation ticks. `clockMs()` is the authoritative
room timeline; every input frame carries the client's estimate of that same
timeline as `InputFrame.clientTimeMs` (0 when the client clock is not synced
yet). `atClientMs` is the safe entry point: it clamps future timestamps to now
and old timestamps to `maxRewindMs` (default 250 ms, configurable per call).


> [!TIP]
> **🏆 Recommended for Maximum Performance: WebAssembly (Rust / Pure C / C++)**
> For competitive 128 Hz authoritative multiplayer rooms, **WebAssembly (Rust / C / C++) is the strongly recommended #1 choice**:
> - **17–20 ns Call Latency**: Cranelift JIT compiled native machine execution (>52M calls/s).
> - **Zero Garbage Collection**: Mathematically flat frame times without GC pauses or jitter.
> - **Hardware Sandbox**: Linear memory bounds protection isolating rooms and host server.
> - **Deterministic 3D Physics**: Native vector math for lag compensation and raycasting.
> 
> *(AssemblyScript is recommended for TypeScript devs; Lua is provided for zero-compile rapid prototyping).*

---

## 🗂️ Language Interfaces Overview

| Language | Interface File | Target Runtime | How Bridge is Handled |
| :--- | :--- | :--- | :--- |
| 🥇 **Rust** *(Recommended)* | [`rust/cyg_room.rs`](./rust/cyg_room.rs) | `wasm32-unknown-unknown` | `export_room!(MyRoom)` macro |
| 🥇 **Pure C** *(Recommended)* | [`c/cyg.h`](./c/cyg.h) | `clang` / `wasi-sdk` / `zig cc` | **Direct Normal C Functions** (`CYG_EXPORT`) |
| 🥇 **C++** *(Recommended)* | [`cpp/cyg.hpp`](./cpp/cyg.hpp) | `clang++` / `wasi-sdk` / `zig c++` | `CYG_ROOM(MyRoom)` macro |
| 🥈 **AssemblyScript** | [`assemblyscript/cyg-server.ts`](./assemblyscript/cyg-server.ts) | `asc` | **100% Automatic** by CYG compiler! |
| 🥉 **Lua** | [`lua/cyg.lua`](./lua/cyg.lua) | Native LuaJIT (`engine-lua`) / WASM | `CYG.Room:extend()` / Procedural |
| **C#** | [`csharp/CYG.cs`](./csharp/CYG.cs) | `.NET 8/9 wasi-wasm` / `NativeAOT` | `[UnmanagedCallersOnly]` static bridge |
| **TypeScript** | [`typescript/cyg.d.ts`](./typescript/cyg.d.ts) | Node / Browser SDK / IntelliSense | Shared typings & contracts |

---

## 🚀 Quick Start Examples

### 1. C++
```cpp
#include "cyg.hpp"

class MyBattleRoom : public cyg::Room {
public:
    void onCreate(int32_t maxClients) override {
        // Init room
    }
    void onJoin(int64_t id, const char* options, int32_t opt_len, const char* auth, int32_t auth_len) override {
        cyg::broadcast("join", 4, nullptr, 0);
    }
    void onLeave(int64_t id, int32_t code) override {}
    void onInput(int64_t id, const uint8_t* data, size_t len) override {}
    uint64_t onTick(double dt) override {
        // Return cyg::pack_delta(delta_buf, delta_len);
        return 0;
    }
};

CYG_ROOM(MyBattleRoom);
```

### 2. Rust
```rust
use cyg_room::{Room, export_room, pack_delta, broadcast};

#[derive(Default)]
pub struct MyBattleRoom {
    max_clients: i32,
}

impl Room for MyBattleRoom {
    fn on_create(&mut self, max: i32) {
        self.max_clients = max;
    }
    fn on_join(&mut self, id: i64, options: &str, auth: &str) {
        broadcast("join", b"ok");
    }
    fn on_leave(&mut self, id: i64, code: i32) {}
    fn on_input(&mut self, id: i64, data: &[u8]) {}
    fn on_tick(&mut self, dt: f64) -> (usize, usize) {
        (0, 0)
    }
}

export_room!(MyBattleRoom);
```

### 3. C# (.NET 8/9 WASI)
```csharp
using System;
using CYG;

public class MyBattleRoom : Room
{
    public override void OnCreate(int maxClients) {
        Log("Battle room created!");
    }
    public override void OnJoin(long id, string options, string auth) {
        Broadcast("join", new byte[] { 1 });
    }
    public override void OnLeave(long id, int code) {}
    public override void OnInput(long id, ReadOnlySpan<byte> data) {}
    public override long OnTick(double dt) => 0;
}

// In Program.cs:
CYGExports.ActiveRoom = new MyBattleRoom();
```

### 4. AssemblyScript
```typescript
import { Room } from "./cyg-server";

export class MyBattleRoom extends Room {
    onCreate(max: i32): void { ... }
    onJoin(id: i64, options: string, auth: string): void { ... }
    onLeave(id: i64, code: i32): void { ... }
    onInput(id: i64, ptr: usize, len: i32): void { ... }
    onTick(dt: f64): i64 { ... }
}
// Zero export boilerplate! The compiler automatically generates the flat ABI bridge.
```

### 5. Pure C (Procedural — Zero Classes)
```c
#include "cyg.h"

// Provides default self-contained heap allocator, on_save, on_load, on_is_full
CYG_DEFAULT_HOOKS

CYG_EXPORT void on_create(int32_t max_clients) {
    // Initialize room
}

CYG_EXPORT void on_join(int64_t id, int32_t opt_ptr, int32_t opt_len, int32_t auth_ptr, int32_t auth_len) {
    cyg_broadcast("join", 4, NULL, 0);
}

CYG_EXPORT void on_leave(int64_t id, int32_t code) {}

CYG_EXPORT void on_input(int64_t id, int32_t ptr, int32_t len) {
    const uint8_t* data = (const uint8_t*)(uintptr_t)ptr;
    // Process input
}

CYG_EXPORT int64_t on_tick(double dt) {
    // Return cyg_pack_delta(delta_buf, delta_len);
    return 0;
}
```

### 6. Lua (Roblox / Defold / Love2D Style)
```lua
local CYG = require("cyg")

local BattleRoom = CYG.Room:extend()

function BattleRoom:onCreate(maxClients)
    self.max_clients = maxClients
    print("[Room] Created room with max clients: " .. maxClients)
end

function BattleRoom:onJoin(id, spawnX, spawnZ)
    self.players[id] = { id = id, x = spawnX, y = 0.0, z = spawnZ }
end

function BattleRoom:onLeave(id)
    self.players[id] = nil
end

function BattleRoom:onInput(id, data)
    -- Process player input data
end

function BattleRoom:onTick(dt)
    -- Fixed tick update (20 - 120 Hz)
    -- Return serialized delta byte string
    return ""
end

return BattleRoom
```
