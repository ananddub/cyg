# Handoff — A2 (runtime-P1)

> Self-contained copy of `docs/P0-P4-EXECUTION-PLAN.md`: shared rules + this
> lane's phase + the cross-cutting sections. Verbatim, so nothing outside this
> file is needed to start work. Master doc stays the source of truth.

---

# cyg-gaming P0 → P4 Execution Plan (agent hand-off)

Written 2026-10-30 on branch `feat/p0-p4-hardening`.

This document is the only context you need. It assumes you have never seen this
repo. Every task lists the file to touch, the current-code fact that makes the
change necessary, and the command that proves it works.

---

## 0. Read this before writing any code

### 0.1 What the product is

`cyg-gaming` is a Rust game-server + backend-as-a-service product with two halves
that talk over an in-process message bus:

| Crate | Role |
|---|---|
| `crates/engine-api` | Axum HTTP/WS control plane. PlayFab-shaped `/v2/*` player API plus a `/console` operator API. Owns SurrealDB. |
| `crates/engine-server` | The authoritative host loop: rooms, ticks, rollback, snapshots, UDP/WebRTC/QUIC netcode. |
| `crates/engine-control` | Boots the runtime, binds the HTTP listener, spawns the host, signal handling, Agones integration. |
| `crates/engine-net` | Transport layer (WS/UDP/QUIC/WebRTC), greeting, origin policy. |
| `crates/engine-bus` | Typed message types shared between the two halves (`engine-bus/src/messages.rs`, `engine-bus/src/telemetry.rs`). |
| `crates/engine-config` | `config.toml` + flat keys + `OX_*` env vars → `ControlConfig` / `GameConfig`. |
| `crates/engine-wasm` | Guest-module runtime, rollback journal, checksums, epoch config. |
| `crates/engine-dashboard` | React 19 + Vite operator console, served at `/dashboard/`. |
| `src/main.rs` (root pkg) | The `cyg-gaming` / `cyg-game` CLI (hand-rolled arg match, no clap). |
| `crates/auto/route`, `crates/auto/di`, `crates/auto/ws` | Proc-macro routing (`#[controller]`/`#[get]`), DI (`#[singleton]`), WS mount. |

Storage is **SurrealDB 3.3.5** (embedded `mem://` / `surrealkv://`, or remote).
There are **no `.surql` files and no migration directory** — schema DDL is
generated from Rust model structs (see §5.1).

### 0.2 Non-negotiables (violating any of these fails review)

1. **Never break a public API path.** Routes are macro-derived and pinned by
   `crates/engine-api/tests/auto_routes/every_migrated_path_is_still_registered.rs:27-48`.
   Any new `#[get]`/`#[post]` must be added to that expected list.
2. **Never change an existing refusal message.** Tests assert exact strings; the
   full protected list is §8. If a change genuinely requires a new string, update
   the test in the same commit and say so in the commit message.
3. **`allow_unverified` is a local-dev hatch only.** `IapConfig::allow_unverified`
   and `SocialConfig::allow_unverified` are set to `true` in test harnesses and
   must stay `false` in every shipped config. A PR that turns them on outside
   `[dev]` contexts is rejected.
4. **500 MB container budget.** No new heavy dependency. Prefer `rustls`
   (`reqwest` in `crates/engine-api/Cargo.toml:67` already uses
   `features = ["json","form","rustls","http2"]`) over `native-tls`/OpenSSL. Do
   not add `prometheus`/`opentelemetry` exporter stacks without measuring; see
   §5.3 for the acceptable minimal path.
5. **Files over ~200 lines become a folder/module.** `crates/engine-api/tests/service_file_sizes.rs`
   enforces this. When you grow a service file, split it (`mod.rs` + siblings).
6. **No comments unless the *why* is non-obvious.** No `// increment i` noise, no
   task references, no docstring blocks.
7. **Do not touch `landingpage/*`** — it has unrelated working-tree changes.
   Do not commit `.archify/`.
8. **Commit per logical group** (`feat(...)`/`fix(...)` imperative subject, short
   body). Do not push.

### 0.3 Commands you will use

The root `Justfile` has **no** `check`/`test`/`clippy`/`fmt` targets (only
`up down restart logs game status list-games sh build rebuild release-local
build-dashboard doctor`). Use raw cargo:

```bash
cargo check -p engine-api  --all-targets        # fastest useful signal
cargo check -p engine-server --all-targets
cargo check -p engine-config --all-targets
cargo test  -p engine-api --lib                 # in-crate suites (src/tests/*)
cargo test  -p engine-api --test gap_player     # one integration binary
cargo test  -p engine-api --test rest_e2e
cargo test  -p engine-api                      # everything for the crate
cargo test  -p engine-server --all-targets
cargo test  --workspace                         # final gate before a commit
cargo clippy --workspace --all-targets
cargo fmt --all
```

Test layout gotcha: integration tests are **directories with a `main.rs`**
(`tests/rest_e2e/`, `tests/gap_http/`, `tests/gap_player/`, `tests/gap_console/`,
`tests/gap_edge/`, `tests/gap_ws/`, `tests/auto_routes/`,
`tests/wasm_map_match_e2e/`). A new `.rs` file inside one of those dirs is
**invisible until you add `mod x;` / `pub use x::*;` to that dir's `main.rs`.**
Standalone binaries: `tests/{moderation,ws_socket,service_file_sizes,surrealkv_budget}.rs`.
The shared harness lives at `tests/harness/mod.rs` and is imported per dir with
`#[path = "../harness/mod.rs"] mod harness_shared;` (`tests/gap_http/main.rs:7-9`).

### 0.4 The two patterns this plan reuses constantly

**Pattern A — fail-closed external verification with a dev hatch.** Already
implemented twice; copy it rather than inventing a third.

- IAP: `crates/engine-api/src/services/iap/verify/mod.rs` →
  `pub(crate) async fn verify_purchase(config: &IapConfig, store: i16, credential: &str, product_id: Option<&str>, transaction_id: Option<&str>) -> Result<VerifiedPurchase, V2Error>`.
  Configured → real HTTP call. Unconfigured + `allow_unverified` →
  `unverified(...)` deterministic `hex_id(credential)`. Unconfigured otherwise →
  per-provider message.
- Social: `crates/engine-api/src/services/auth/verify/mod.rs` →
  `pub(crate) async fn profile(config: &SocialConfig, kind: Kind, token: &str) -> Result<Profile, V2Error>`,
  `Profile { id, display_name, avatar_url }`, `Kind::{Apple,Facebook,Google,Steam}`,
  `Kind::parse(name)` (lowercase only), `Kind::label()`.
- Config side: `crates/engine-config/src/iap.rs`, `crates/engine-config/src/social.rs`,
  each with `*_configured()` predicates and `allow_unverified`.

**Pattern B — one SurrealQL transaction for multi-write consistency.** There is
no transaction closure API on the datastore, so writes are a single script:

```rust
// crates/engine-api/src/db/exec/open.rs:102
pub async fn query_transaction<T>(&self, surql: &str, params: ...) -> Result<Vec<T>, DbError>
```

`query_transaction` scans the statement results for the **first non-empty array**,
so `BEGIN`/`LET`/`COMMIT` (which return nothing) still yield the guarded `IF`
rows. Only existing caller: `apply_wallet_change` at
`crates/engine-api/src/repositories/iap/queries.rs:177` (used by
`crates/engine-api/src/services/console/account/wallet.rs`). Copy that shape for
every escrow/grant/delete task below. `None` result ⇒ refusal mapped by the
service, e.g. `V2Error::failed_precondition("A currency balance cannot go negative.")`.

### 0.5 Adding a table (needed by P0/P1/P2/P4)

1. New model in `crates/engine-api/src/repositories/models/<name>.rs`:
   `#[derive(SurrealRecord)] #[table("<name>", schemaless)] #[index(...)]`
   (see `repositories/models/users.rs:3`).
2. Export from `repositories/models/mod.rs`.
3. Add the type to the `ddl()` list `crates/engine-api/src/db/schema.rs:19-46`.
4. **Bump the pinned count** in `crates/engine-api/src/db/schema.rs:60`
   (`assert_eq!(...count(), 26)` → the new number). This test also asserts every
   table is schemaless and every index uses `IF NOT EXISTS` — do not add
   `DEFINE FIELD`/`SCHEMAFULL`.
5. Schema is applied at boot: `crates/engine-api/src/app/api.rs:14` →
   `ensure_schema` (`db/schema.rs:49`) → `db/exec/raw_all.rs:7 apply()`.

### 0.6 Adding a route (the smallest copyable chain)

`controllers/notification.rs:18-29` (`#[controller("/v2/notification")]` +
`#[get("")]` + `Params(req): Params<ListQuery>`) →
`services/notification.rs:34-60` (`#[singleton]` service, `q(state).await?`) →
`repositories/notification/queries.rs:110` (`list_notifications_paged`).
State access helpers: `crates/engine-api/src/services/deps.rs:8 q(state)`,
`:15 blobs(state)`. Query object: `crates/engine-api/src/repositories/db.rs:11
Queries` with `all/one/opt/exec/scalar/create` (`:24-79`), `params!` (`:82`),
`ts()` (`:113`), `affected()` (`:102`).
Param structs are named `Insert*Params` / `List*Params`
(`repositories/iap/types.rs:3,14,26,33`).

Auth extractors (`crates/engine-api/src/app/middleware.rs`): `Authed` (`:12`,
player session), `server_key` (`:65`, Basic), `server_key_or_console` (`:83`,
service-to-service OR operator, never a player token), `ConsoleAuthed` (`:103`,
plus `require_write()` `:110`, `require_admin()` `:120`).

---

## 1. Agent assignment and ordering

Four agents, one per lane. Lanes are parallel **except** where stated.

| Agent | Lane | Phases | Depends on |
|---|---|---|---|
| **A1 – Trust** | API security, accounts, moderation | P0 (all remaining) | nothing |
| **A2 – Runtime** | engine-server authority, lifecycle, snapshots, shutdown | P1 | A1 for the event-ingest auth only (soft) |
| **A3 – Live ops** | economy, quests, marketplace, webhooks, notifications, flags | P2 | A1 done (moderation/report tables), A2 done (match results source) |
| **A4 – Platform** | migrations, deploy, observability, CI, dashboard/DX | P3 + P4 | A1/A2/A3 landmarks for metrics + config keys |

Hard sequencing rules:

- A3 must not start `match results` before A2 ships the gameplay-event sink
  (task 2.2), or it has no data source.
- A4's `schema_version` work (5.1) must land before A3 adds more than two new
  tables, so the new tables arrive through the versioned path.
- A1's social wiring (1.3) is **in flight right now** — see the exact state
  before touching those files.

Branch/commit hygiene: each agent works on `feat/p0-p4-hardening`, commits per
task or per tightly-coupled pair, never pushes, never amends a published commit.
`git status` before any destructive command; the repo has real in-progress work.

---


---

## 3. PHASE 1 — authoritative runtime (Agent A2)

### 3.1 Lifecycle timers: make the hardcoded ones real config

Facts: `GameConfig` is `crates/engine-config/src/game.rs:37`
(`tick_rate, max_clients, max_rooms, port, quic_port, aoi, rollback, control`;
defaults `:78-106`, tick_rate 30, max_rooms 1024). `ControlConfig`
(`src/control.rs:3`) has exactly one lifecycle knob: `seat_ttl_secs` (default
300, `:51`, parsed `parse/toml.rs:141`, flat/env `parse/flat.rs:48`).
In the host loop `crates/engine-server/src/host.rs` (`run()` at `:518`, main
`loop` `:983-2335`, tick gate `:1909-1913` with 3-tick catch-up cap, per-room
tick `:1950-1974`) the timers are literals:
`room.reap_check(Duration::from_secs(30))` at **`:1943`**, UDP peer idle
**3000 ms at `:2292`**, pinned-plane UDP idle **10 s at `plane/udp.rs:59`**,
`OX_TICK_HZ` default 128 at `plane/udp.rs:142`. `room_max_seconds`,
`reap_idle_secs`, `hibernate`, `persistent_rooms` do not exist.

Work:

1. Add to `GameConfig` (`crates/engine-config/src/game.rs`):
   `room_max_seconds: u64` (0 = unlimited), `reap_idle_secs: u64` (default 30),
   `peer_idle_ms: u64` (default 3000), `seat_hold_secs: u64` (default 120,
   reconnect seat hold), `persistent_rooms: Vec<String>` (never reaped).
   Clamp in `Default`/parse (`max_rooms`-style): values must be sane, not panic.
2. Parse them: `crates/engine-config/src/parse/toml.rs` `[game]` block,
   `parse/flat.rs` flat keys, `parse/mod.rs` env loop
   (`OX_ROOM_MAX_SECONDS`, `OX_REAP_IDLE_SECS`, `OX_PEER_IDLE_MS`,
   `OX_SEAT_HOLD_SECS`, `OX_PERSISTENT_ROOMS` comma-split).
3. Replace the literals `host.rs:1943` and `host.rs:2292`
   (`plane/udp.rs:59`) with config reads. Keep the 1 s telemetry publish (`:2325`)
   and 500 ms wasm hot-reload (`:2385`) as they are.
4. `closes_at` deadline: compute per room from `room_max_seconds` at room create,
   store on the room struct, check in the same housekeeping pass; a room at
   deadline takes the dead-room disposal + snapshot path already at
   `host.rs:2311-2320`.
5. Tests: unit tests in `crates/engine-config` for clamping + env parsing;
   `crates/engine-server/tests/host_services_test.rs` for "empty room is reaped
   at `reap_idle_secs`", "persistent room is not reaped", "duration deadline
   closes an active room".

### 3.2 Room state machine: Created → Active → Hibernating → Closed

Facts: hibernation exists only as `room.reap_check(...)` (`host.rs:1937-1948`)
and dead-room disposal at `host.rs:2311-2320`; there is no named state and the
snapshot is **in-memory only** (`GameServer::save_snapshot` →
`persistent_snapshots: HashMap<String, Vec<u8>>` at `rooms.rs:2181/2209`, restore
at `:2282`; process-local, lost on restart). Guest hooks exist:
`GameRoom::export_snapshot()` `rooms.rs:1648` (wasm `on_save`, storage
permission-gated `:432-433`) and `import_snapshot()` `rooms.rs:1687` (`on_load`).

Work:

1. Add `RoomState { Created, Active, Hibernating, Closed }` on the room, with an
   explicit transition table; empty rooms go `Active → Hibernating` (skip their
   sim tick, 0% CPU) and `Hibernating → Active` on the next input.
2. `Closed` terminal path: `export_snapshot()` → persist → close frames with
   code 1000 → dispose. Broadcast guest `onDispose` (today nothing happens on
   close — see §3.5).
3. Expose the state in `StatusSnapshot` (`crates/engine-bus/src/telemetry.rs:24`)
   as a per-state room-count field, and in `/status`
   (`crates/engine-api/src/services/status/*`, `controllers/status.rs:15`).
4. Tests: `crates/engine-server/tests/host_services_test.rs` state transitions;
   assert no tick work happens while `Hibernating`.

### 3.3 DB-backed snapshots + room handoff

Facts: the table **already exists** — `repositories/models/room_persistent_snapshot.rs:4`
(`#[table("room_persistent_snapshot", schemaless)]`, `snapshot_bytes`) with
queries `save_room_snapshot` / `load_room_snapshot` at
`crates/engine-api/src/repositories/inventory/queries.rs:133,154`. They have no
caller outside `inventory/tests.rs:101-125`, and **engine-server holds no DB
handle**. There is **no room registry / node assignment anywhere** (grep
`room_registry|node_assign|handoff` in engine-control/engine-api: zero hits);
`SimCommand::BindPeer { room }` (`engine-bus/src/messages.rs:95`) keys on room
name only.

Work:

1. Move snapshot persistence across the bus instead of opening a DB handle in
   the host: new `SimCommand`/`RoomCommand` variant pair
   `SaveSnapshot { room, bytes, seq }` and `SnapshotSaved { room, ok }` in
   `crates/engine-bus/src/messages.rs`, consumed by the API-side pump
   (`crates/engine-api/src/infra/pump.rs:7`, 250 ms poll) which calls the
   existing `save_room_snapshot`.
2. Replace the `persistent_snapshots` HashMap as the source of truth: keep it as
   a write-through cache, but make the rehydrate path
   (`rooms.rs:2282`) fall back to `load_room_snapshot` on a cold start.
3. Add `room_registry` table (§0.5): `room_id`, `node_id`, `game_id`, `state`,
   `peers`, `tick`, `snapshot_seq`, `update_time`; writer = host via the same bus
   path, reader = `/v2/match` and the console. Handoff = "registry row exists for
   another node" ⇒ refuse local create, redirect/attach via the existing seat
   ticket machinery (`crates/engine-api/src/infra/seats/{registry.rs:9 HmacSha256, verify.rs}`).
4. Tests: unit round-trip for the wire types; `crates/engine-api/tests/` gap test
   that a snapshot written by the host is readable via `load_room_snapshot`, and
   that a room id owned by another node is refused.

### 3.4 Gameplay events → control plane (the missing `sys_event`)

Facts: `sys_event`/`game_event` have **zero hits repo-wide**. The reverse channel
today is only `StatusSnapshot` (`engine-bus/src/telemetry.rs:24`) via
`bus.status_tx`, published 1/s by `Recorder::maybe_publish` (call sites
`host.rs:2364`, `sim.rs:340/476`) and consumed by `pump()`. Guest events leave via
`host_send` (`rooms.rs:553`) → `OutboundMessage { event, target, payload }` →
`drain_outbox()` (`rooms.rs:2122`) → `flush_room_outbox()` (`host.rs:376`) and
are **wire-only: nothing persists them**. `SimCommand::Event` (`messages.rs:95`)
is control→sim client push only (`sim.rs:269`).

Work:

1. Add a second sender beside `status_tx`: `event_tx: mpsc::Sender<SimEvent>` in
   the bus with `SimEvent { room, tick, event, target, payload, seq }`.
2. Emit from `flush_room_outbox` (`host.rs:376`) a mirror of each outbound
   message whose `event` matches a per-game `record_events` allowlist (new
   `GameConfig` field, §3.1) — bounded queue, drop-oldest with a counter, never
   block the tick.
3. Consume in the API-side pump and persist to a new `gameplay_event` table
   (§0.5): `id, room, game_id, event, player_id, payload, tick, create_time`,
   indexed on `(room, create_time)`. Retention: `event_ttl_days` config, sweeper
   in the same housekeeping pass as §3.1.
4. Expose `GET /v2/match/{id}/events` behind `Authed` (participant-scoped) and
   console list behind `ConsoleAuthed`.
5. Tests: bus round-trip unit test; `crates/engine-server/tests/host_services_test.rs`
   for allowlist filtering + backpressure (bounded queue drops, tick continues);
   API gap test for the persisted row and the participant-scoped read.

### 3.5 Server-authoritative input validation

Facts: input enters at `TransportEvent::Packet` in the host main loop (WebRTC
`host.rs:1204-1621`, UDP lane `:1626-1779`, QUIC `Inbound::Input` `:1813`) and is
dispatched by **`netcode::apply_input()` at `crates/engine-server/src/netcode.rs:103`**
(call sites `host.rs:1546, 1572, 1825`; `sim.rs:321`). Existing guards: seq
newest-wins journal (`netcode.rs:101-236`), payload size
(`MAX_INBOUND_PAYLOAD_SIZE`, checked `host.rs:1213`), sender stamp anti-spoof for
events (`host.rs:1507`). **`InputRateLimiter` exists (`netcode.rs:433`, token
bucket) and `rtc_input_rates`/`udp_input_rates` are created (`host.rs:836-837`)
but only ever `.remove()`d — never `.allow()`-checked on the RTC path; only
`udp_join_rates` (`host.rs:1717`) actually gates.** No tick-window or content
validation.

Work:

1. Wire the existing limiter: call `.allow()` on both the RTC and UDP input paths
   in `host.rs` before `apply_input`; on refusal, drop + count (new
   `StatusSnapshot` field `input_dropped`).
2. Add authority checks in `apply_input`: reject inputs whose `tick` is outside
   `[current_tick - rollback.max_replay_ticks, current_tick + 2]`; reject
   `player_id` not seated in that room (seat authority lives in
   `crates/engine-server/src/plane/seats.rs` and the seat-ticket registry);
   enforce a per-input `buttons/axis` shape bound before the guest sees it.
3. Add a per-room, per-player input budget derived from the tick rate, so a
   client cannot post 500 inputs/tick.
4. Tests: `netcode.rs` inline `#[cfg(test)]` cases for out-of-window tick,
   unseated player, oversized content, over-budget; one `crates/engine-server/tests/`
   case proving a flooded peer is rate-limited and its inputs are counted as
   dropped while other peers keep simulating.

### 3.6 Tick watchdog default-on + desync verification wiring

Facts: `crates/engine-wasm/src/rollback.rs` has FNV-1a `compute_checksum_64`
(`:23`), `CheckpointFrame { checksum }` (`:48`) and **`DesyncReport {
expected_checksum, client_checksum }` (`:53`) which is never constructed or
consumed anywhere in the repo**. Per-tick checksums are computed
(`RollbackBuffer::record` `:102-119`) and readable (`get_checksum(tick)` `:173`);
checkpoints are used for replay (`netcode.rs:163`). Epoch interruption exists but
is off: `crates/engine-wasm/src/config.rs:12 enable_epoch_interruption: bool`
(default `false`; docs `:9-11` require external `set_epoch_deadline`/
`increment_epoch`, and there is **no production caller** — only
`engine-wasm/tests/csharp_wasm_test.rs:22`). Deterministic RNG already exists:
`crates/engine-core/src/rng.rs RoomRng` (+ `engine-core/tests/rng_determinism.rs`).

Work:

1. Enable the epoch deadline in the host: call `set_epoch_deadline`/`increment_epoch`
   around each room tick (`host.rs:1950-1974`, and `plane/sim.rs:361-392`), with
   `enable_epoch_interruption` defaulting to **true** and a
   `epoch_deadline_ticks`/`tick_budget_ms` config knob for opt-out.
2. On budget overrun, log a warn + record a watchdog event; keep the room alive
   but advance the tick (do not kill a room on one slow tick). Add a
   `max_ticks_per_frame` catch-up cap consistent with the existing 3-tick
   (`host.rs:1913`) and `MAX_CATCHUP_TICKS = 2` (`sim.rs:35`).
3. Desync verification: add an inbound opcode/message carrying a client's
   `(tick, checksum)`; compare against `get_checksum(tick)`; on mismatch
   **construct** `DesyncReport` and (a) count it on `StatusSnapshot`,
   (b) persist to a new `desync_report` table via the §3.4 event bus, (c) expose
   `GET /console/desync` behind `ConsoleAuthed`.
4. Tests: `engine-wasm` unit tests that a mismatched client checksum produces a
   `DesyncReport` with both fields; a host test that a guest exceeding the epoch
   budget is interrupted without poisoning the room; keep
   `rng_determinism.rs` green (no behaviour change to `RoomRng`).

### 3.7 SIGTERM graceful drain

Facts: `shutdown_signal()` is `crates/engine-control/src/plane.rs:52-53` and is
**`tokio::signal::ctrl_c()` only — no SIGTERM** (the `unix` feature is unused).
It gates the Axum accept halt (`run.rs:39`) and Agones retirement
(`run.rs:82-91`). `src/main.rs:916` runs `engine_server::host::run()` on the main
thread and **the sim loop has no shutdown check at all** — rooms die with the
process, no drain, no `export_snapshot` flush, no guest `onDispose` broadcast.

Work:

1. Add `tokio::signal::unix(signal(SignalKind::terminate()), ...)` alongside
   `ctrl_c` in `plane.rs:52` (keep the same async fn signature so `run.rs:39/82`
   are untouched).
2. Introduce a `Drain` state on the host: on signal, set a flag checked in the
   main loop; stop admitting new peers/rooms (UDP/WebRTC/QUIC accept paths at
   `host.rs:1204`, `:1626`, `:1813`), broadcast "server closing", keep ticking for
   `drain_grace_secs` (new `GameConfig` knob, default 15).
3. During drain, force `export_snapshot()` per room through the §3.3 bus path,
   then close WS sessions with code 1000 and exit.
4. Agones: retirement must call the same drain before
   `crate::agones::shutdown()` (`run.rs:82-91`).
5. Tests: a `crates/engine-server/tests/` integration test that a drain request
   produces snapshot saves for open rooms and refuses new peers; a small unit test
   that the drain flag is observed within one tick.

### 3.8 Replay / match history

Facts: **nothing stores finished matches.** Every `replay` hit is rollback
re-simulation (`netcode.rs:101-236`). `leaderboard` / `leaderboard_record` exist
as models (`repositories/models/player_inventory_item.rs:20,49`) with DTO/util
surface (`src/dto/leaderboard.rs`, `src/utils/leaderboard.rs`) but there is no
`match_history` table anywhere, and `data/` holds no match records.

Work:

1. New `match_history` table (§0.5): `id, game_id, room_id, started_at,
   ended_at, end_reason (closed|duration|empty|drain|error), peak_players,
   ticks, snapshot_seq, result (object), summary`.
2. Write it from the same housekeeping/disposal pass that already calls
   `export_snapshot` (`host.rs:2311-2320`) through the §3.4 bus path — do not
   open a DB handle in the host.
3. Replay artifact = last `snapshot_bytes` + the room's `gameplay_event` rows.
   Serve `GET /v2/match/{id}/history` (`Authed`, participant-scoped) and
   `GET /console/match/{id}/replay` (`ConsoleAuthed`); keep the existing
   `GET /v2/match` (`controllers/match.rs:21`) semantics unchanged.
4. Retention config `match_history_ttl_days`; sweeper in the §3.1 pass.
5. Tests: `crates/engine-server/tests/host_services_test.rs` for the end-reason
   mapping; API gap test for both read paths and participant scoping.

---


---

## 7. Cross-cutting verification matrix

Run these at each commit boundary; a phase is not done until its row is green.

| Scope | Command |
|---|---|
| Any `engine-api` edit | `cargo check -p engine-api --all-targets && cargo test -p engine-api --lib` |
| Player-facing API change | `cargo test -p engine-api --test gap_player && cargo test -p engine-api --test rest_e2e` |
| Console/dashboard API change | `cargo test -p engine-api --test gap_console && cargo test -p engine-api --test gap_edge` |
| WS change | `cargo test -p engine-api --test gap_ws && cargo test -p engine-api --test ws_socket` |
| Route added/removed | `cargo test -p engine-api --test auto_routes` |
| New table/model | `cargo test -p engine-api --lib db` (pinned table-count assertion `db/schema.rs:60`) |
| Runtime/host change | `cargo test -p engine-server --all-targets` |
| Control/boot/signal change | `cargo test -p engine-control --all-targets` |
| Config change | `cargo test -p engine-config --all-targets` |
| File-size / memory budget | `cargo test -p engine-api --test service_file_sizes && cargo test -p engine-api --test surrealkv_budget` |
| Dashboard change | `pnpm -C dashboard build && pnpm -C dashboard lint` |
| Before every push-worthy commit | `cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace` |

---

## 8. Protected strings and status codes (do not change)

Refusal messages asserted by the suite. `refusal(...)` in the gap tests compares
exact text; the in-crate `src/tests/*` helpers compare the same strings against a
numeric code (`INVALID_ARGUMENT`=400, `UNAUTHENTICATED`=401, `FAILED_PRECONDITION`
/`DATA_LOSS` per mapping in `crates/engine-api/src/error/`).

Server key / authenticate surface (`tests/gap_player/provider_auth_surface.rs`,
`src/tests/authenticate/social.rs`):

- `"Server key required"` (401) — every `/v2/account/authenticate*` verb without
  a Basic key.
- `"Server key invalid"` (401) — wrong key.
- `"Apple authentication is not configured."` (400) and
  `"Steam authentication is not configured."` (400) — config check happens
  **before** the token check for these two.
- `"Facebook access token is required."` / `"Google access token is required."`
  (400) — checked **before** the profile call.
- `"Could not authenticate Facebook profile."` /
  `"Could not authenticate Google profile."` (401) — unconfigured provider for
  these two is 401, not 400. (`crates/engine-api/src/utils/vendor.rs:3/:7/:11`
  are the sources: `"{provider} authentication is not configured."`,
  `"Could not authenticate {provider} profile."`,
  `"Authentication provider not found: {lowercased}"`.)
- `"Signed Player Info for a Facebook Instant Game is required."` (400) and
  `"Error extracting the Facebook Instant Game player ID or validating the
  signature"` (500 / DATA_LOSS) — note the missing trailing period is load-bearing.
- `"GameCenter access credentials are required."` (400),
  `"GameCenter bundle ID is required."` (400, bundle is checked first),
  `"Could not authenticate GameCenter profile."` (401), and the
  `GameCenter {player ID|public key URL|salt|signature} is required.` /
  `GameCenter timestamp is required.` family
  (`services/auth/social.rs:37`).
- `"Username invalid, no spaces or control characters allowed."` (400) — must be
  reached before any vendor call (`services/auth/validate.rs:17`); also
  `"Username invalid, must be 1-128 bytes."`.
- `"User account banned."` (`utils/auth.rs:11`).
- `"Username must be 3-20 characters and free of profanity."`
  (`services/auth/local/device.rs:27`) — keep this string when centralizing the
  check in §2.2.

Wallet / IAP (`src/tests/iap.rs`, `tests/*/iap_surface.rs`):

- `"A currency balance cannot go negative."` (500/`FAILED_PRECONDITION` mapping
  from `query_transaction` returning `None`).
- `"Apple purchase verification is not configured, set [control.iap] apple_shared_secret."`
  and `"Google purchase verification is not configured, set [control.iap]
  google_service_account and google_package_name."`, plus
  `"Purchase verification is not implemented for store {other}."`
  (`services/iap/verify/mod.rs`).
- `"Apple rejected the receipt (status N)."` (non-zero Apple status).
- `"Google reports purchase state N."` (non-zero `purchaseState`).
- Unconfigured-store IAP gap tests still expect **200 with environment
  "UNKNOWN"** for fake receipts, because every harness sets
  `allow_unverified: true`. Store codes: 0 Apple, 1 Google, 2 Huawei,
  3 Facebook Instant, 4 Samsung; environment codes 1=SANDBOX, 2=PROD, else
  "UNKNOWN".

Dashboard / console (`tests/gap_edge/dashboard_api.rs`,
`tests/gap_edge/dashboard_socket.rs`,
`crates/engine-api/src/controllers/dashboard/ws.rs`):

- `"Console token required"` (401) — WS handshake with no `?token=` and no
  bearer; must be refused **before** tenant scoping.
- `/metrics` requires server key or console; `/status` stays open (health-like).

Delete / GDPR (`tests/gap_http/player_surface.rs:196-215`,
`tests/rest_e2e/account_surface.rs:411-442`):

- `DELETE /v2/account` must leave the account row count at `0` and produce
  exactly `1` `user_tombstone` row (`assert_eq!(tombstoned, 1)`). §2.3 keeps
  these true while covering more tables.

Schema (`crates/engine-api/src/db/schema.rs:58-60`):

- The table-count assertion (`26` today) is a guard, not a spec: bump it in the
  same commit that adds a model, and never make a table `SCHEMAFULL` or an index
  non-`IF NOT EXISTS`.

---

## 9. Risks and mitigations

1. **Test-contract breakage is the top risk.** Every refusal message and status
   code above is asserted twice (in-crate `src/tests/*` and `tests/gap_*`). Read
   the test before designing the change; if the design and the contract conflict,
   the contract wins unless the user agrees otherwise in the commit message.
2. **Harness drift.** New `ControlConfig` fields must be added to every hand-built
   state literal: `tests/harness/harness.rs`, `tests/rest_e2e/harness.rs`,
   each `tests/gap_*/harness.rs`, and `src/tests.rs::v2_state`. Prefer
   `..Default::default()` in harnesses so future fields do not churn.
3. **`allow_unverified` leaking.** It appears in two configs and in three
   harnesses; the §5.6 `doctor` check must fail loudly on any deployment where it
   is enabled (rule 3).
4. **Host-loop latency.** All P1 additions (event mirror, rate limiting, epoch
   enforcement, drain) run inside `host.rs`'s tick. Keep every new pass O(rooms)
   with bounded queues, drop-oldest counters instead of blocking, and reuse the
   existing housekeeping cadence (`host.rs:1937-1948`, `:2311-2320`, `:2325`).
5. **Dependency weight.** rustls-only outbound HTTP, no new exporter/3D/TLS
   crates without a recorded size delta; `service_file_sizes.rs` and
   `surrealkv_budget.rs` are the automated tripwires.
6. **Schema convergence.** Adding versioned migrations on top of generated DDL
   (§5.1) must not break fresh `mem://` boots used by ~100 integration tests —
   always run `ensure_schema` first, migrations second, and test a cold boot.
7. **Untracked in-flight work.** The social-auth files listed in §2.1 are already
   on disk and **have not been compiled yet**. Do not re-create them; finish the
   wiring.

---

## 10. Definition of done, per phase

- **P0** — no `/v2` or `/console` write path is unauthenticated; social tokens are
  cryptographically verified (or explicitly refused) before a session exists; a
  GDPR erase removes every user-owned row and kills live sessions; a self-export
  returns the full record set; reports/moderation are enforced on every text
  ingress. `cargo test --workspace` green, no `allow_unverified` outside harnesses.
- **P1** — lifecycle timers are config-driven and clamped; rooms have a named
  state machine ending in a persisted snapshot; gameplay events, match history and
  desync reports reach the database through the bus; player input is rate-limited
  and validated server-side; SIGTERM drains with snapshots flushed; the epoch
  watchdog is on by default with a config escape.
- **P2** — every economy write is idempotent and transactional; quests/seasons/
  dailies/markets/wagers/subscriptions/flags/quotas/notifications/webhooks exist
  with operator CRUD, tenant scoping, retention sweeps and tests.
- **P3** — schema is versioned and migratable with a fail-closed boot; backup and
  restore are scripted, CLI-exposed and drilled; metrics, structured logs and
  request ids are live; k8s + Agones manifests with `/readyz` and TLS terminate
  outside the app; CI runs fmt/clippy/tests/dashboard; secrets rotate without mass
  logout and `doctor` audits them.
- **P4** — OpenAPI is served and CI-checked against the SDK; the console gains
  logs, player timeline, economy dashboard, 3D map preview, rules editor and
  config history; the CLI gains `backup restore migrate schema-status openapi
  "secrets audit" "config diff" "rules check" "logs tail"`;
  `pnpm -C dashboard build && lint` green.
