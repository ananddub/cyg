# CYG client SDKs — the cyg-core architecture

Every client platform shares ONE protocol core: **`crates/cyg-core-rs`**.
It IS the SDK core — schema descriptor parsing, OXS2 snapshot/patch decode,
delta unwrap, and input encoding all live there. Platforms keep only
transport (sockets/WebRTC), reconnection, voice and REST glue. The old
per-language decoders were REMOVED; games do not change their public surface
(`CygClient`, cyg.js `Room` events, addon signals are unchanged).

## Typesafe TypeScript core (`sdk/client/core-ts`)

`sdk/client/web/cyg.js` is now a BUILD ARTIFACT, not the source of truth.
The source lives in **`sdk/client/core-ts`** (`@cyg/core-ts`):

- `src/core/engine.js` — the client engine (transports, reconnect, voice,
  prediction, REST) verbatim, on its way to a module-by-module split.
- `src/room.ts` — `TypedRoom<TState, TMessages>`: compile-time checked
  `send`/`onMessage`/`state` driven by generated schema types.
- `src/schema.ts` — `schemaFromTable()` attaches a generated runtime table
  to a room.
- Types come from the engine: `cyg schema-codegen <dir> --lang ts` emits
  `cyg_schema_generated.d.ts` (typed `RoomState`/`GameState`/`GameRoom`
  interfaces + runtime schema tables) — the same single schema that feeds
  the C#/GDScript/C++/Lua codegen.

Build (`just core-ts-build`): esbuild → `dist/cyg.cjs` (UMD drop-in,
byte-compatible export shape with the old cyg.js), `dist/cyg-core.mjs` (ESM)
and `dist/*.d.ts`. Tests (`just core-ts-test`): the core-ts suite plus the
full web test suite against the freshly built bundle.

```
                       cyg-core-rs (Rust, one codebase)
                              │             │
              wasm32 build    │             │  native cdylib
              (C ABI exports) │             │  (libcyg_core_rs.so/.dll/.dylib)
                              ▼             ▼
        ┌──────────────────┐   ┌──────────────────────────────┐
        │ Web (wasm)       │   │ Unity C# (P/Invoke) ·        │
        │ cyg.js + cyg-    │   │ Unreal C++ (runtime load) ·  │
        │ core-web.mjs     │   │ Godot GDExtension            │
        └──────────────────┘   └──────────────────────────────┘
```

## Status (what is verified)

| Platform | Core integration | Verification |
|---|---|---|
| Rust (any host) | `crates/cyg-core-rs` | ✅ unit tests + **live-server parity test** (`tests/live_parity.rs`: real captured deltas) |
| Web | `cyg.js` routes ALL schema decode/input packing through `cyg_core_rs.wasm`; `cyg-core-web.mjs` is the minimal wasm-only room | ✅ e2e loadtest (8 clients, 0 errors) + **node wasm parity test** (`/tmp/wasm_parity.mjs` pattern) + fixture capture |
| Unity (C#) | `Runtime/CygClient.cs` builds a `CygRoom` (P/Invoke core) from the host descriptor and applies every delta there; `CygDeltaDecoder` is a thin row shim over the core | ✅ `test/` dotnet native parity + `Tools~/CygRuntime.Tests` (29 checks) |
| Godot | `sdk/client/godot/cyg-godot/` GDExtension; `addons/cyg/cyg_room.gd` delegates ALL decode to it (the GDScript OXS2 decoder was deleted) | ✅ gdext 0.5 build + headless Godot 4.7 tests: `test/schema_decoder_test.gd` (22 checks) + `test/cyg_relay_client_test.gd` (104 checks) |
| Unreal | `sdk/client/unreal/` C++ plugin (dynamic loading) | ⚠️ source-only (no UE SDK on build machine); loads `libcyg_core_rs.so/.dll/.dylib` at runtime |

Removed decoders (they live only in `cyg-core-rs` now):
- Web: cyg.js `parseSchemaDescriptor` + `_readSchemaField/_readSchemaElement/
  _decodeEntity/_applySchemaFrame/_applyCollectionSection/_applyWorldSection`
  (~700 lines) and the hand-packed `_packSchemaInput`.
- Unity: `CygDeltaDecoder`'s TLV/envelope decoder (now a shim over the core).
- Godot: `addons/cyg/cyg_room.gd` `_read_schema_element/_read_schema_field/
  _decode_entity/_apply_schema_frame/_apply_collection_section/
  _apply_world_section`, `cyg_schema_def.gd parse_descriptor`, and the
  unused second decoder `addons/cyg/cyg_schema.gd` (deleted).

## The C ABI (stable contract)

`crates/cyg-core-rs/src/wasm_ffi.rs` (wasm) and `src/ffi.rs` (native) export
the same surface; guard `cyg_abi_version() == 1` in bindings:

| Export | Purpose |
|---|---|
| `cyg_create(descriptor, len)` | parse `cyg.schema.bin` → handle |
| `cyg_apply_payload(handle, bytes, len)` | apply one room-data frame (envelope strip included) |
| `cyg_state_ready(handle)` | kind-1 full snapshot applied |
| `cyg_entity_count(handle)` / `cyg_entity_id_at(handle, pos)` | iterate rows |
| `cyg_entity_count_in / cyg_entity_id_at_in / cyg_get_f64_in / cyg_get_string_in / cyg_get_bool_in` | per-collection reads |
| `cyg_get_f64_by_index / get_i64_by_index / get_bool_by_index / get_string_by_index / get_string_len` | typed reads (primary collection) |
| `cyg_field_index(handle, name, len)` | name → schema-field index |
| `cyg_schema_count / schema_name / schema_hash / schema_version / schema_field_count / schema_field_name / schema_field_kind / schema_field_flags / schema_field_element_kind` | schema metadata — bindings build their typed mirrors from these, no second parser |
| `cyg_collection_count / collection_name / collection_schema_hash` | collection metadata |
| `cyg_root_field_count / root_field_name / root_field_kind / root_schema_name / root_schema_version / cyg_get_root_f64 / cyg_get_root_string / cyg_get_root_bool` | root scalars (kind-3/4 world frames) |
| `cyg_encode_schema_input(handle, seq, i64s, ..., out, cap, out_len)` | schema-lane input frame |
| `cyg_last_acked_seq(handle)` / `cyg_last_server_tick(handle)` | ack/tick tail of the last applied payload (prediction rollback ack) |
| **prediction / rollback** (see the section below) | `cyg_pred_create / pred_free / pred_capture / pred_restore / pred_predict / pred_record / pred_reconcile / pred_pending_count / pred_pending_seq / pred_pop_input / pred_divergence` |
| `cyg_free(handle)` | release |

The GDExtension (`sdk/client/godot/cyg-godot/src/lib.rs`) exposes the same
surface as `CygRoom` methods (`load_descriptor`, `apply_payload`,
`schema_*`, `collection_*`, `get_f64_in`, `get_root_*`, `encode_schema_input`).

## Build commands

```bash
# native lib (Unity/Unreal/Godot)
cargo build -p cyg-core-rs --release
# → target/release/libcyg_core_rs.so

# web wasm (plain C ABI, no wasm-bindgen glue needed)
cargo build -p cyg-core-rs --target wasm32-unknown-unknown --release --features wasm
# → target/wasm32-unknown-unknown/release/cyg_core_rs.wasm
#   (ship it next to cyg.js as cyg_core_rs.wasm — cyg.js loads it on join;
#    in the browser set the CYG_CORE_WASM_URL global to override the path)

# godot extension
cargo build --release --manifest-path sdk/client/godot/cyg-godot/Cargo.toml
# → registered for the Godot project via addons/cyg/bin/cyg_godot.gdextension

# all tests (14 unit incl. prediction/rollback + live parity)
cargo test -p cyg-core-rs
```

## Verifying each platform

```bash
cd bench-game-2donly && ../target/release/cyg &     # server up

# web (loadtest uses cyg.js over the wasm core)
node bench/loadtest.mjs --url http://127.0.0.1:8297 --room arena \
    --clients 8 --seconds 8 --hz 30 --move

# fixtures + parity
node bench/capture_cyg_deltas.mjs --url http://127.0.0.1:8297 \
    --room arena --seconds 10 --out crates/cyg-core-rs/tests/fixtures
cargo test -p cyg-core-rs --test live_parity
node /tmp/wasm_parity.mjs target/wasm32-unknown-unknown/release/cyg_core_rs.wasm

# unity
cp sdk/client/unity/CygRoom.cs sdk/client/unity/test/ && \
    cp target/release/libcyg_core_rs.so sdk/client/unity/test/bin/Debug/net8.0/
cd sdk/client/unity/test && dotnet run                       # C# NATIVE PARITY OK
cd sdk/client/unity/Tools~/CygRuntime.Tests && dotnet run -c Release

# godot (headless)
godot --headless --path sdk/client/godot -s res://test/schema_decoder_test.gd
godot --headless --path sdk/client/godot -s res://test/cyg_relay_client_test.gd
```

## Adding a new platform

1. Bind the C ABI above (~150-200 lines: transport + handle glue).
2. Assert `cyg_abi_version() == 1`.
3. Feed every room-data frame into `cyg_apply_payload`; poll entity reads;
   build any typed mirror from the `cyg_schema_*` metadata getters.
4. Parity-check against `tests/fixtures/` (same bytes must produce the same
   values cyg.js decoded — see tests/live_parity.rs as the template).

## Known limits of the core surface

- Array fields decode and are stored, but the C ABI/GDExtension do not expose
  element reads yet (bindings hydrate arrays as empty).
- Optional-field presence on hydration: numeric fields read NaN when absent,
  strings read "" — callers distinguish via the `optional` flag in metadata.

## Client-side prediction / rollback (`predict.rs`)

Closes the biggest gap vs Colyseus (its `@colyseus/sdk` `src/predict/`
reconciler). The design is inspired by Colyseus's MIT-licensed
`RollbackController` (rewind-and-replay, per-seq predicted-state history,
EMA/peak drift telemetry) and reimplemented over OXS2 entities:

- **Game-agnostic.** Entities are `Vec<Value>` in schema field order; the game
  supplies the input→state step (`InputApplier::apply(&mut entity, &[i64])`,
  e.g. `x += dir * speed`). The core never knows what fields mean.
- **Server truth + ack in one frame.** Every payload envelope tail
  `[acked_seq u16][server_tick u32]` is now exposed (`RawFrame.acked_seq` →
  `ClientCore::last_acked_seq()` → `cyg_last_acked_seq`), so the authoritative
  patch and the rollback ack arrive together. Reconcile uses the post-patch
  entity values as the restore point (Colyseus's `adoptTruth` role).
- **Divergence without float tricks.** Numeric fields measure
  `|predicted − authoritative|` (max per reconcile feeds an EMA α=0.1 + peak
  decay 0.9 — Colyseus `drift.ts` semantics: steady nonzero `ema` = real
  divergence, `peak` spikes = jitter); bools/strings/arrays compare exactly.

### The loop (Rust hosts)

```rust
use cyg_core_rs::predict::{Predictor, InputApplier};

let mut pred = Predictor::new(0);               // collection slot
pred.capture_pre_state(core.state());           // rollback baseline (spawn)

// per input (zero latency): buffer + apply locally + record history
pred.predict(seq, input_values.clone())?;
applier.apply(&mut entity, &input_values);
pred.record_history(core.state(), entity_id, seq)?;

// per payload: core.apply_payload(bytes) applies the authoritative patch;
// then reconcile = measure divergence at the ack, drop acked inputs, adopt
// server truth:
let out = pred.reconcile(core.state_mut(), entity_id, core.last_acked_seq());
// rewind-and-replay: replay every still-pending input on top of the truth
for input in pred.pending() {
    applier.apply(&mut entity, &input.values);
    pred.record_history(core.state(), entity_id, input.seq);
}
let s = pred.stats(); // last_max_drift / ema / peak / confirmed / predicted / ...
```

### The C ABI (game-driven replay — the applier cannot cross the ABI)

The same symbols exist in `ffi.rs` (native) and `wasm_ffi.rs` (wasm):

| Export | Purpose |
|---|---|
| `cyg_pred_create(core, collection_slot)` → pred handle | one predictor per collection (usually the players collection) |
| `cyg_pred_capture(pred, core)` | snapshot every entity of the collection (rollback baseline); `cyg_pred_restore` writes it back |
| `cyg_pred_predict(pred, seq, i64s, len)` | buffer an input (same raw-i64 encoding as `cyg_encode_schema_input`) |
| `cyg_pred_record(pred, core, entity_id, seq)` | record the entity's CURRENT values as the predicted state for `seq` (after a live apply, and after every replayed apply) |
| `cyg_pred_reconcile(pred, core, entity_id)` → n | measure divergence at the last acked seq, drop acked inputs, ADOPT server truth; returns how many inputs to replay |
| `cyg_pred_pending_count / pending_seq / pop_input` | peek the pending inputs (they drain only via acks) |
| `cyg_pred_divergence(pred, out f64[7])` | `[last_max_drift, ema, peak, confirmed, predicted, acked_total, mismatch_events]` — the debug-HUD feed |

JS/Unity loop sketch:

```js
// spawn
const pred = cyg_pred_create(core, 0);
cyg_pred_capture(pred, core);
// per input
cyg_pred_predict(pred, seq, i64s);
myApplier.apply(entity, i64s);            // game code, zero latency
cyg_pred_record(pred, core, entityId, seq);
// per payload
core.applyPayload(bytes);                 // authoritative patch + ack tail
const replay = cyg_pred_reconcile(pred, core, entityId);
for (let i = 0; i < replay; i++) {
    const s = cyg_pred_pending_seq(pred, i);
    cyg_pred_pop_input(pred, i, buf, buf.length, lenOut);
    myApplier.apply(entity, buf);         // replay on top of server truth
    cyg_pred_record(pred, core, entityId, s);
}
cyg_pred_divergence(pred, stats);         // HUD: stats[2] peak, stats[1] ema
```

Tests (`cargo test -p cyg-core-rs`): deterministic sim converges with
divergence 0 and pending drained after the ack; server-side wind makes the
replay diverge (drift 1.5 reported, truth adopted) and converges once the
client's applier matches; ack spanning multiple inputs, stale/seq-jump acks,
empty-pending reconcile, rollback restore, and a full loop driven over the C
ABI. Deferred (Colyseus parity backlog): presentation-side smoothing/
interpolation (`Predictor.ts` lerp/damped), `ctx.memo`, predictedEvents /
predictedSpawns — those belong in the bindings, on top of these numbers.
