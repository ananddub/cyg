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
| 🥇 **Pure C** *(Recommended)* | [`c/cyg.h`](./c/cyg.h) | `clang` / `wasi-sdk` / `emscripten` | **Direct Normal C Functions** (`OX_EXPORT`) |
| 🥇 **C++** *(Recommended)* | [`cpp/cyg.hpp`](./cpp/cyg.hpp) | `clang` / `emscripten` / `wasi-sdk` | `CYG_ROOM(MyRoom)` macro |
| 🥈 **AssemblyScript** | [`assemblyscript/cyg.ts`](./assemblyscript/cyg.ts) | `asc` | **100% Automatic** by CYG compiler! |
| 🥉 **Lua** | [`lua/cyg.lua`](./lua/cyg.lua) | Native LuaJIT (`engine-lua`) / WASM | `CYG.Room:extend()` / Procedural |
| **C#** | [`csharp/CYG.cs`](./csharp/CYG.cs) | `.NET 8 NativeAOT` / `wasi-wasm` | `[UnmanagedCallersOnly]` static bridge |
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
    void onJoin(int64_t id, double x, double z) override {}
    void onLeave(int64_t id) override {}
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
use cyg_room::{Room, export_room, pack_delta};

#[derive(Default)]
pub struct MyBattleRoom {
    max_clients: i32,
}

impl Room for MyBattleRoom {
    fn on_create(&mut self, max: i32) {
        self.max_clients = max;
    }
    fn on_join(&mut self, id: i64, spawn_x: f64, spawn_z: f64) {}
    fn on_leave(&mut self, id: i64) {}
    fn on_input(&mut self, id: i64, data: &[u8]) {}
    fn on_tick(&mut self, dt: f64) -> (usize, usize) {
        (0, 0)
    }
}

export_room!(MyBattleRoom);
```

### 3. C# (.NET 8 NativeAOT)
```csharp
using CYG;

public class MyBattleRoom : Room
{
    public override void OnCreate(int maxClients) { ... }
    public override void OnJoin(long id, double spawnX, double spawnZ) { ... }
    public override void OnLeave(long id) { ... }
    public override void OnInput(long id, ReadOnlySpan<byte> data) { ... }
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
    onJoin(id: i64, spawnX: f64, spawnZ: f64): void { ... }
    onLeave(id: i64): void { ... }
    onInput(id: i64, ptr: usize, len: i32): void { ... }
    onTick(dt: f64): i64 { ... }
}
// Zero export boilerplate! The compiler automatically generates the flat ABI bridge.
```

### 5. Pure C (Procedural — Zero Classes)
```c
#include "cyg.h"

// Provides default on_alloc, on_save, on_load, on_is_full
OX_DEFAULT_HOOKS

OX_EXPORT void on_create(int32_t max_clients) {
    // Initialize room
}

OX_EXPORT void on_join(int64_t id, double spawn_x, double spawn_z) {}

OX_EXPORT void on_leave(int64_t id) {}

OX_EXPORT void on_input(int64_t id, int32_t ptr, int32_t len) {
    const uint8_t* data = (const uint8_t*)(uintptr_t)ptr;
    // Process input
}

OX_EXPORT int64_t on_tick(double dt) {
    // Return ox_pack_delta(delta_buf, delta_len);
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
