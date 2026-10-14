# 🎮 OpenOxide Multi-Language Room SDK Interfaces

OpenOxide is language-agnostic. Game rooms compile into WebAssembly and execute inside a sandboxed Wasmtime instance with native ~17ns invocation speed.

Developers write authoritative multiplayer game logic using pure, high-level object-oriented or idiomatic room classes without needing to write low-level WebAssembly C-ABI functions.

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
| 🥇 **Rust** *(Recommended)* | [`rust/openoxide_room.rs`](./rust/openoxide_room.rs) | `wasm32-unknown-unknown` | `export_room!(MyRoom)` macro |
| 🥇 **Pure C** *(Recommended)* | [`c/openoxide.h`](./c/openoxide.h) | `clang` / `wasi-sdk` / `emscripten` | **Direct Normal C Functions** (`OX_EXPORT`) |
| 🥇 **C++** *(Recommended)* | [`cpp/openoxide.hpp`](./cpp/openoxide.hpp) | `clang` / `emscripten` / `wasi-sdk` | `OPENOXIDE_ROOM(MyRoom)` macro |
| 🥈 **AssemblyScript** | [`assemblyscript/openoxide.ts`](./assemblyscript/openoxide.ts) | `asc` | **100% Automatic** by OpenOxide compiler! |
| 🥉 **Lua** | [`lua/openoxide.lua`](./lua/openoxide.lua) | Native LuaJIT (`engine-lua`) / WASM | `OpenOxide.Room:extend()` / Procedural |
| **C#** | [`csharp/OpenOxide.cs`](./csharp/OpenOxide.cs) | `.NET 8 NativeAOT` / `wasi-wasm` | `[UnmanagedCallersOnly]` static bridge |
| **TypeScript** | [`typescript/openoxide.d.ts`](./typescript/openoxide.d.ts) | Node / Browser SDK / IntelliSense | Shared typings & contracts |

---

## 🚀 Quick Start Examples

### 1. C++
```cpp
#include "openoxide.hpp"

class MyBattleRoom : public openoxide::Room {
public:
    void onCreate(int32_t maxClients) override {
        // Init room
    }
    void onJoin(int64_t id, double x, double z) override {}
    void onLeave(int64_t id) override {}
    void onInput(int64_t id, const uint8_t* data, size_t len) override {}
    uint64_t onTick(double dt) override {
        // Return openoxide::pack_delta(delta_buf, delta_len);
        return 0;
    }
};

OPENOXIDE_ROOM(MyBattleRoom);
```

### 2. Rust
```rust
use openoxide_room::{Room, export_room, pack_delta};

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
using OpenOxide;

public class MyBattleRoom : Room
{
    public override void OnCreate(int maxClients) { ... }
    public override void OnJoin(long id, double spawnX, double spawnZ) { ... }
    public override void OnLeave(long id) { ... }
    public override void OnInput(long id, ReadOnlySpan<byte> data) { ... }
    public override long OnTick(double dt) => 0;
}

// In Program.cs:
OpenOxideExports.ActiveRoom = new MyBattleRoom();
```

### 4. AssemblyScript
```typescript
import { Room } from "./openoxide";

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
#include "openoxide.h"

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
local OpenOxide = require("openoxide")

local BattleRoom = OpenOxide.Room:extend()

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
    -- Fixed tick update (20 - 128 Hz)
    -- Return serialized delta byte string
    return ""
end

return BattleRoom
```
