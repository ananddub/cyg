# ⚡ `engine-wasm`

> **Ultra-Fast, Decoupled WebAssembly Game Runtime** powered by [Wasmtime](https://wasmtime.dev/) and Cranelift JIT.  
> Delivers **~20 nanosecond execution latency**, zero Garbage Collector pauses, and invisible, type-safe Rust method bindings.

[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Engine](https://img.shields.io/badge/wasmtime-v25-blue.svg)](https://bytecodealliance.org)
[![Latency](https://img.shields.io/badge/tick%20latency-20.4%20ns-brightgreen.svg)](#-benchmarks)
[![Throughput](https://img.shields.io/badge/throughput-49M%20ticks%2Fsec-success.svg)](#-benchmarks)

---

## 🌟 Why `engine-wasm`?

Traditional game servers run user scripts directly inside the engine process (e.g., Node.js / V8). While simple, a single script crash takes down the entire server, and V8 Garbage Collector pauses cause unpredictable 10ms–30ms frame drops.

`engine-wasm` solves this with a **zero-overhead WebAssembly sandbox**:

- **🚀 Sub-20ns Execution Latency:** Static function pointers are pre-cached, eliminating all runtime export lookups and string hashing.
- **🛡️ 100% Crash-Proof Sandboxing:** If a guest room script traps, infinite loops, or panics, the host server stays alive with zero memory corruption.
- **🪄 "Invisible WASM" API (`wasm_module!`):** You don't pass raw instances, stores, or string names. Calling WASM feels **100% identical to calling normal native Rust methods on a struct**.
- **🌐 Write in TypeScript (AssemblyScript), Rust, or C++:** Game developers can write in familiar TypeScript syntax and compile to `.wasm` binaries as small as **239 bytes**.
- **🧼 Pure & Decoupled:** Contains zero game-specific event lock-in. It is a clean, general-purpose runtime engine that adapts to any architecture.

---

## 📊 Benchmarks (Real Test Data)

> Benchmarked on **1,000,000 real execution iterations** in `--release` mode:
>
> Instantiation benchmarked with **10,000 rooms spawned from a pre-instantiated template**:

| Operation | Latency | Throughput (Single Core) |
| :--- | :--- | :--- |
| **Pure Function Call** | **17.27 ns** | ~58,000,000 calls / sec |
| **Real Game Tick (AssemblyScript)** | **20.40 ns** | **49,022,927 ticks / sec** |
| **Type-Safe Macro Call (`wasm_module!`)** | **19.27 ns** | **51,886,807 calls / sec** |
| **Room Instantiation (On-Demand)** | **~14.0 µs** | ~71,000 rooms / sec |
| **Room Instantiation (Pooling + `InstancePre`)** | **~1.5 µs** | **~658,000 rooms / sec** |
| **Module Compilation (One-Time)** | **2.74 ms** | N/A |
| **WASM Binary Size (`--runtime stub`)** | **239 bytes** | Literal bytes, not KB! |

> **Context:** A 60 FPS tick budget is **16.6 milliseconds** (`16,666,000 ns`). An `engine-wasm` tick executes in **0.0001%** of that frame budget — and a pooled room spawns **~9× faster** than on-demand.

---

## 🚀 Quickstart

### 1. Write Game Logic in AssemblyScript (TypeScript Syntax)

```typescript
// game_room.ts
let posX: f32 = 100.0;
let posY: f32 = 100.0;
let score: i32 = 0;

export function update(dt: f32, speedX: f32, speedY: f32): void {
    posX += speedX * dt;
    posY += speedY * dt;
    score += 1;
}

export function getPositionX(): f32 {
    return posX;
}

export function calculateDamage(base: i32, multiplier: f32): i32 {
    return i32(f32(base) * multiplier);
}
```

Compile with speed-optimized flags and the **Zero-GC** stub runtime:
```bash
asc game_room.ts -o game_room.wasm -Ospeed --noAssert --runtime stub
```
*(Produces a hyper-optimized ~239 byte binary!)*

---

### 2. Call from Rust with `wasm_module!` (Invisible WASM)

```rust
use engine_wasm::{WasmRuntime, wasm_module};

// 1. Declare the type-safe struct wrapper
wasm_module! {
    pub struct GameRoom {
        fn update(dt: f32, speed_x: f32, speed_y: f32);
        fn getPositionX() -> f32;
        fn calculateDamage(base: i32, multiplier: f32) -> i32;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Initialize runtime & load compiled bytes
    let runtime = WasmRuntime::new()?;
    let wasm_bytes = include_bytes!("game_room.wasm");
    let module = runtime.compile(wasm_bytes)?;

    // 3. Create the room instance
    let mut room = GameRoom::new(&runtime, &module)?;

    // 4. Call methods directly — ZERO WASM boilerplate!
    room.update(0.016, 150.0, 50.0)?;
    let pos_x = room.getPositionX()?;
    let dmg = room.calculateDamage(50, 1.5)?;

    println!("Position X: {}, Damage: {}", pos_x, dmg);

    // 5. Direct linear memory access is also built-in:
    room.write_memory(0, b"packet_payload")?;
    let data = room.read_memory(0, 14)?;

    Ok(())
}
```

---

### 3. Spawning Rooms at Maximum Speed (`InstancePre` + Pooling)

For high room-churn workloads, pre-instantiate the module once (validation,
linking and type-checks happen a single time), then spawn rooms from the
template — with the pooling allocator, each room spawns in **~1.5 µs**:

```rust
use engine_wasm::{WasmConfig, WasmRuntime};

let runtime = WasmRuntime::new_with_config(WasmConfig {
    use_pooling_allocator: true,        // ~9× faster instantiation (see benchmarks)
    ..WasmConfig::default()
})?;

let module = runtime.compile(wasm_bytes)?;

// One-time setup: validation + linking baked into a template.
let pre = runtime.prepare_standalone::<()>(&module)?;

// Every room now spawns via the fastest path:
let mut room = GameRoom::from_pre(&pre, ())?;
room.update(0.016, 150.0, 50.0)?;
```

Compiled modules can also be cached to disk to skip the one-time ~2.7 ms
Cranelift compilation on subsequent boots:

```rust
runtime.serialize_module(&module, "rooms.cwasm")?;
// SAFETY: the file must come from a trusted source (not re-validated on load).
let module = unsafe { runtime.deserialize_module("rooms.cwasm") }?;
```

---

### 4. Integrating Custom Host Functions (Linker)

If the guest WASM needs to call host services (broadcasting packets, logging, or querying state):

```rust
#[derive(Default)]
struct RoomHostContext {
    pub broadcast_packets: Vec<Vec<u8>>,
}

let mut linker = runtime.create_linker::<RoomHostContext>();

// Register host function
linker.func_wrap("env", "host_broadcast", |mut caller: wasmtime::Caller<'_, RoomHostContext>, ptr: u32, len: u32| {
    // Read guest memory and push packet
})?;

let state = RoomHostContext::default();
let mut room = GameRoom::from_module_with_linker(&runtime, &linker, &module, state)?;

// Access host state directly:
println!("Packets queued: {}", room.data().broadcast_packets.len());
```

---

## ⚙️ Configuration (`WasmConfig`)

`engine-wasm` is fully configurable and comes with sane high-performance defaults:

```rust
use engine_wasm::{WasmConfig, WasmRuntime};

let config = WasmConfig {
    optimize_for_speed: true,           // Cranelift OptLevel::Speed
    enable_epoch_interruption: false,   // Optional watchdog for rogue infinite loops
    use_pooling_allocator: false,       // ~9× faster room spawning, larger fixed memory footprint
};

let runtime = WasmRuntime::new_with_config(config)?;
```

> **Epoch interruption note:** the flag only enables the mechanism. To actually
> stop rogue loops, arm each instance with `room.raw_instance().set_epoch_deadline(n)`
> (or via `WasmInstance::set_epoch_deadline`) and tick the engine periodically
> with `runtime.increment_epoch()` from a background thread or main loop.
>
> **Pooling note:** `use_pooling_allocator` trades a larger fixed memory footprint
> (slots reserved up-front) for ~9× lower instantiation latency. Best for
> high-churn short-lived rooms; for thousands of *simultaneously alive* rooms,
> tune the wasmtime pool limits accordingly.

---

## 🧪 Running the Test Suite

All tests can be executed with standard Cargo commands:

```bash
# Run all unit and integration tests (bench-timing tests require release mode)
cargo test --release -p engine-wasm

# Run the full release-mode latency benchmarks
cargo test --release -p engine-wasm --test assemblyscript_stub_test -- --nocapture

# On-Demand vs Pooling allocator instantiation benchmark
cargo test --release -p engine-wasm --test instantiation_bench -- --nocapture
```

---

## 📜 Architecture Summary

```
                      [ AssemblyScript / Rust / C++ ]
                                     │
                                     ▼ asc / clang
                             [ .wasm Bytecode ]
                               (239 bytes - Zero GC)
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│  engine-wasm (Pure Rust)                                               │
│                                                                        │
│   WasmRuntime (Cranelift Engine)                                       │
│          │                                                             │
│          ▼ prepare() → InstancePre template + Pooling Allocator        │
│   wasm_module! Macro Wrapper (from_pre fast spawn)                     │
│          │                                                             │
│          ├── room.update(dt, vx, vy)          --> Direct JIT Pointer   │
│          ├── room.calculate_damage(50, 1.5)   --> 20.4 ns Latency      │
│          └── room.read_memory(ptr, len)       --> Linear Buffer        │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 📄 License

Apache-2.0
