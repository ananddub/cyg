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

## 2. PHASE 0 — trust boundaries (Agent A1)

Ship P0 before anything else. Each task is independently committable.

### 2.1 IN FLIGHT — finish social authentication (Apple / Google / Facebook / Steam)

Current state, verified:

- `crates/engine-api/src/services/auth/social.rs` is **80 lines of stubs**:
  `:9 pub fn token(profile: &str, raw: &str, requested_username: &str) ->
  Result<Json<SessionResponse>, V2Error>` returns
  `"{profile} access token is required."` for an empty token, otherwise
  **unconditionally** `Err(vendor::profile(profile))`. No verification, no user
  lookup, no user creation, anywhere in the file.
- Real verifiers exist but are **unreachable** (zero call sites outside the
  module): `src/services/auth/verify/{mod,apple,google,facebook,steam}.rs` —
  Google via `tokeninfo` (`aud` ∈ `google_client_ids`, `exp` in the future, `sub`
  required), Apple via JWKS + RS256 (`DecodingKey::from_rsa_components`, audience
  = `apple_client_id`, issuer `https://appleid.apple.com`), Facebook via
  `debug_token` (`data.is_valid`, `data.app_id` match, `data.user_id`), Steam via
  `ISteamUserAuth/AuthenticateUserTicket/v1` (`response.params.result == "OK"`,
  `steamid`).
- `SocialConfig` exists at `crates/engine-config/src/social.rs:11` with
  `*_configured()` (`:52-66`) and `allow_unverified` (`:30`), mounted at
  `crates/engine-config/src/control.rs:19`. TOML `[control.social]` parsing and
  `OX_SOCIAL_*` env vars are already written
  (`parse/toml.rs`, `parse/flat.rs`, `parse/mod.rs:164-202`).
- Repository layer is **already complete** — no DB work needed:
  `repositories/auth/queries.rs` has `get_user_by_{google,apple,facebook,steam}_id`
  and `create_user_with_{google,apple,facebook,steam}`, with matching
  `CreateUserWith*Params` in `repositories/auth/types.rs`
  (`{id, username, ...}` plus `display_name`/`avatar_url` for google/facebook).
- Session minting helper: `crates/engine-api/src/utils/auth.rs:31
  session(uid, username, created, vars)` → `infra::jwt::mint_token_pair` (`:37`)
  → `SessionResponse { created, token, refresh_token }`. Ban check at login lives
  in `utils/auth.rs:11 existing()` → `"User account banned."`.
- Tenant stamping + creation shape to copy:
  `crates/engine-api/src/services/auth/local/device.rs` (`get_device_by_id :11`,
  `create_device :31`, `stamp_user :32`).
- Handlers to rewire: `src/services/auth/service/mod.rs` — `apple :73` and
  `steam :84` currently short-circuit to `vendor::unconfigured` **before**
  reaching `social.rs`; `facebook :103`, `google :114`,
  `facebook_instant_game :125`, `game_center :136`, `provider :147`; `authorize`
  `:23` = server key + `query.create()?`.

Work to do:

1. Convert `services/auth/social.rs` into a folder `services/auth/social/{mod.rs,...}`
   if it exceeds ~200 lines after the rewrite (rule 5). Make `token`
   `pub async fn token(state: &ApiState, kind: verify::Kind, token: &str,
   requested_username: &str, create: bool, vars: &HashMap<String,String>)`.
2. Order of checks **is load-bearing** (tests pin it):
   1. `validate::username(requested_username)?` — so `bad[]name` is refused
      **before** any network call.
   2. Empty-token refusal for the token providers, message
      `"{Label} access token is required."` from `Kind::label()`
      (`Facebook`/`Google`). Keep Apple/Steam checking configuration **first**:
      `POST /v2/account/authenticate/apple` with `{}` must yield
      `"Apple authentication is not configured."` (400), not the empty-token
      message.
   3. `verify::profile(&state.control.social, kind, token).await?` — with the
      existing fallback messages from `crates/engine-api/src/utils/vendor.rs`:
      `:3 "{provider} authentication is not configured."` (FAILED_PRECONDITION,
      used by Apple/Steam), `:7 "Could not authenticate {provider} profile."`
      (UNAUTHENTICATED, used by Google/Facebook). **Preserve this asymmetry.**
   4. `get_user_by_<provider>_id` → found ⇒ `utils::auth::existing(user)?` then
      `session(...)` with `created: false`.
   5. Not found ⇒ if `!create` return `V2Error::not_found(...)` matching the
      existing "account not found" wording used by `local/device.rs`; else
      moderate the username (§2.2), `create_user_with_*`,
      `services::games::tenant::stamp_user`, `session(..., created: true)`.
3. Declare the module: `crates/engine-api/src/services/auth/mod.rs` currently
   contains only `pub mod local; mod service; pub mod social; pub mod validate;`
   — **add `mod verify;`** (the five verify files are on disk but not in the
   module tree). Then replace the two `vendor::unconfigured` short-circuits in
   calls gated by config presence.
4. Surface the config: add a commented `[control.social]` block to `config.toml`
   and to `crates/engine-config/src/templates.rs`, worded like the IAP block:
   unconfigured providers refuse with 400/401, and `allow_unverified` is local
   development only.
5. Test harnesses must set `social: SocialConfig { allow_unverified: true, ..SocialConfig::default() }`
   next to the existing `iap: IapConfig { allow_unverified: true, ... }` at
   `tests/harness/harness.rs`, `tests/rest_e2e/harness.rs`,
   and `src/tests.rs::v2_state`.

Do-not-break list for this task (all currently passing, all must keep passing):
`src/tests/authenticate/social.rs:5,27,67` and
`tests/gap_player/provider_auth_surface.rs:3` (strings and status codes listed
verbatim in §8).

Verification:

```bash
cargo check -p engine-api --all-targets
cargo test  -p engine-api --lib authenticate
cargo test  -p engine-api --lib social
cargo test  -p engine-api --test gap_player provider
cargo test  --workspace
```

Add new tests in the existing style (mock axum on an ephemeral port, no new dev
deps): point `*_url` config fields at the mock server and assert a real
Google/Apple/Facebook/Steam token creates an account and returns a session.

Commit: `feat(auth): verify Apple, Google, Facebook and Steam tokens before opening a session`.

- Baseline as of this writing: `cargo check -p engine-api --all-targets` is
  **clean** (finished in ~10s), but only because `verify/` is not in the module
  tree — `engine-config` with `SocialConfig` and all three parse paths compile,
  and the five verify files have never been type-checked. Expect the first real
  errors immediately after you add `mod verify;`.

### 2.2 Moderation: content reports and enforcement

Facts: there is **no** `src/services/moderation/`; moderation is the single flat
module `crates/engine-api/src/moderation.rs` (`is_clean :3`, `clean_text :10`,
`sanitize_json :16`, `is_valid_username :30` = trimmed length 3–20 **and**
rustrict-clean). Only two enforcement sites exist:
`services/auth/local/device.rs:26` (device-create only) and
`services/channel/messages/send.rs:32`. `services/auth/validate.rs:17 username()`
checks control chars/`[]`/128 bytes only, so **email, custom-id and social
signups currently bypass profanity filtering**. Ban is operator-only
(`services/console/account/write.rs:14 ban_account`, `:47 unban_account`, via
`users.disable_time`) and enforced only at login (`utils/auth.rs:11`).

Work:

1. New table `content_report` (§0.5): `id`, `reporter_id`, `reported_id`,
   `reason` (enum-ish string: `harassment|cheating|exploit|inappropriate_content|other`),
   `details`, `path` (optional — message/channel/storage key), `status`
   (`open|actioned|dismissed`), `create_time`, `update_time`, index on
   `reported_id` and on `status`.
2. Player API `POST /v2/reports` (or `POST /v2/account/report`, pick one and add
   it to `tests/auto_routes/...:27-48`) behind `Authed`; `GET /v2/reports` list
   for operators behind `ConsoleAuthed` + `require_write()`;
   `PATCH` status transition behind `ConsoleAuthed` + `require_write()`.
3. Run `moderation::sanitize_json` on **all** free-text ingress, not just channel
   messages: account `PUT /v2/account` display name
   (`controllers/account.rs:35`), user metadata, storage values, inventory
   display strings, guild/group names (`services/group/*`), and every signup
   path — call `moderation::is_valid_username` from
   `services/auth/validate.rs::username` so the check is centralized, keeping the
   existing message `"Username must be 3-20 characters and free of profanity."`
4. Enforcement record: new `user_infraction` table (`user_id`, `kind`,
   `note`, `expires_at`, `issued_by`), and a mute (chat-send denial) check inside
   `services/channel/messages/send.rs` before the sanitize call.
5. Tests: `tests/gap_player/report_surface.rs` (register in `tests/gap_player/main.rs`)
   asserting reporter auth, reason validation, operator list/transition, and the
   new centralized username rule for email + social signup.

### 2.3 GDPR: complete erase and self-service export

Facts: `services/account/delete.rs:9 delete_account` does exactly two things —
`create_tombstone` (`:15`) and `delete_account` (`:16`). The repo method
`repositories/account/queries.rs:83 delete_account` removes `user_device`,
`message` (sender_id only), `notification`, `users_notes`, `wallet_ledger`,
`storage`, `storage_index`, `user_edge`, `users`. **It misses user-owned rows in
`purchase`** (`repositories/models/player_inventory_item.rs:110`),
**`subscription`** (`:58` region), **`player_inventory_item`** (`:4`),
**`leaderboard_record`** (`:49`, key column `owner_id` `:61`),
**`group_edge`** (`repositories/models/console_acl_template.rs:97`),
**`room_persistent_snapshot`** (`:4`), and `message` rows where the user is the
recipient. It does **not** revoke sessions (contrast
`services/console/account/write.rs:34 state.sessions.remove_all(id)` and `:36
kick_user`) and is **not atomic**. There is **no player self-export route at all**;
the reusable shape is the operator export
`services/console/account/export.rs:17 export_account` (claims
`require_write()` at `:23`, row cap `EXPORT_ROWS = 1000` at `:14`, DTO
`src/dto/console/account/export.rs:10 AccountExport`) behind
`controllers/console/account/routes.rs:74-90`.

Work:

1. Extend `delete_account` (repo) to cover every missed table above, in one
   `BEGIN TRANSACTION … COMMIT` script via `query_transaction` (§0.4 Pattern B),
   tombstone written inside the same transaction.
2. Add session revocation + kick to `services/account/delete.rs`
   (`state.sessions.remove_all(uid)`, `kick_user`) — a deleted account must not
   keep a live JWT session.
3. New player route `GET /v2/account/export` behind `Authed`, returning the same
   record families as `AccountExport` **plus** purchases, subscriptions,
   inventory items, leaderboard records, group memberships, notifications, and
   messages in both directions. Reuse the queries; do not fork the DTO.
4. New operator route `POST /v2/account/delete` behind
   `server_key_or_console` for service-driven erasure (same service function).
5. Tests: extend `tests/gap_http/player_surface.rs:196-215` (which already
   asserts `rows == 0` for the account row and a `user_tombstone` count) to assert
   each newly covered table is empty for that user id, and add
   `tests/rest_e2e/account_surface.rs` coverage for the export document shape and
   for post-delete session invalidation.

### 2.4 P0 residual audit (small, one commit)

- Confirm no remaining `/v2/*` write is behind only `Params` with no extractor;
  grep `controllers/` for `#[post(` and list the routes that still take neither
  `Authed`, `server_key`, `server_key_or_console` nor `ConsoleAuthed`, then fix
  or document each. `GET /status` stays open by design (`controllers/status.rs:15`);
  `/metrics` is now behind `server_key_or_console` (`:56`).
- `services/games/schema.rs:10 ensure_tenant_schema` is a **no-op stub** — either
  implement tenant scoping or delete the dead call at `app/api.rs:14-17`.

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

## 4. PHASE 2 — backend / live-ops (Agent A3)

### 4.1 Idempotent economy

Facts: `insert_purchase` (`repositories/iap/queries.rs:12`) and
`insert_subscription` (`:64`) are already idempotent (UPSERT on natural key);
`upsert_item_definition` (`:61`) too. Non-idempotent writes:
`apply_wallet_change` (`:177`) — the ledger id is a fresh `Uuid::new_v4()` from
`services/console/account/wallet.rs:53`, so a double POST double-grants;
`grant_inventory_item` (`repositories/inventory/queries.rs:67`, plain `CREATE`);
`create_notification` (`:4`); `create_tombstone`/`delete_account`. Zero hits for
`idempotency`, `request_id`, `dedupe`, `ON CONFLICT`.

Work:

1. Add an `Idempotency-Key` header (or `?idempotencyKey=`, match the PlayFab-ish
   style already used in this API) accepted by wallet writes, inventory grants,
   purchase recording and notification create.
2. New `idempotency_key` table (§0.5): `key, actor, scope, request_hash,
   response, create_time`, unique index on `(actor, scope, key)`. Insert-first
   semantics: a duplicate key returns the stored response, it does not re-run the
   write. Prune older than `idempotency_ttl_days` in the P1 sweeper.
3. Thread the key through `apply_wallet_change` so the `wallet_ledger` row uses a
   deterministic id derived from the key (replace the fresh `Uuid::new_v4()` at
   `services/console/account/wallet.rs:53`); keep the negative-balance guard.
4. Make `grant_inventory_item` `UPSERT`-shaped for stackable definitions
   (`update_inventory_quantity` `repositories/inventory/queries.rs:91` is the
   existing path to reuse) and CREATE-once for non-stackable instances.
5. Tests: replay-the-same-key-returns-the-same-response for every write above; a
   concurrent double-submit test asserting exactly one ledger row.

### 4.2 Quests, seasons, daily rewards

Facts: zero hits for `quest|season|daily|achievement|titledata|statistic`. The
closest analogue for per-player state is `services/console/setting.rs:11
list_settings / :16 update_setting / :48 delete_setting` on the `Setting` model.
Player-scoped stats have no storage today; leaderboard writes exist
(`services/leaderboard/*`) but nothing accumulates counters.

Work:

1. Tables: `quest_definition` (`game_id, key, title, goal_kind, goal_amount,
   reward, season, active_from, active_to`), `player_quest` (`user_id, quest_key,
   progress, state (available|in_progress|completed|claimed), update_time`,
   unique `(user_id, quest_key)`), `season` (`game_id, key, starts_at, ends_at,
   reward_track`), `player_stat` (`user_id, stat, value, update_time`).
2. Definitions authored by operators: `POST/PUT/DELETE /console/quests`
   (`ConsoleAuthed` + `require_write()`), mirroring the shape of
   `controllers/console/setting.rs`.
3. Progress ingest: consume `SimEvent`s from §3.4 (event name → stat counter),
   which is why 4.2 follows 3.4. Update `player_quest.progress`, mark complete,
   grant rewards through the **idempotent** path from 4.1.
4. Daily rewards: `POST /v2/account/daily/claim` behind `Authed`, gated on
   UTC-day boundary stored on the user; second claim in the same day refuses.
5. Tests: quest lifecycle from event → progress → claim → reward, daily claim
   idempotency across a day boundary, season window gating (outside window =
   refused).

### 4.3 Marketplace, trades and wager escrow

Work:

1. Tables `market_listing` (`id, seller_id, item_id, price_currency, price,
   state (open|sold|cancelled|expired), create_time`), `wager` (`id, room,
   players, currency, amount_each, state (held|settled|refunded), winner_room_data`).
2. Escrow **must** be one `query_transaction` script (§0.4 Pattern B) that
   simultaneously debits the seller/bettors and holds into a system wallet, with
   the negative-balance guard; settlement is a second transaction (release /
   split), refund is a third. Never two separate round trips.
3. Routes `POST /v2/marketplace/listings`, `POST /v2/marketplace/listings/{id}/buy`,
   `POST /v2/marketplace/listings/{id}/cancel`, `POST /v2/wager/create`,
   `POST /v2/wager/{id}/settle` (server key only — a player must never settle).
4. Validate the item belongs to the seller (`player_inventory_item` owner column)
   before listing; move ownership inside the same transaction.
5. Tests: escrow debit+hold atomicity (injected failure leaves no half-state),
   double-buy refuses, refund returns exactly the held amount, settle requires the
   server key.

### 4.4 Subscription enforcement (not just recording)

Facts: `services/iap/subscriptions.rs` records only — `subscription_apple/google/
huawei` (`:20,30,39`) → `record_subscription` (`~:120`), list `:46`,
by-product `:77`. `expire_time` is stored (`:130
body.expire_time.or(verified.expire_time).unwrap_or_else(epoch)`) and is
**never compared to `Utc::now()` anywhere**.

Work:

1. Add `is_active(sub: &Subscription, now) -> bool` (state not cancelled-or-expired
   **and** `expire_time > now`) in `services/iap/subscriptions.rs`; keep the
   stored epoch default so historical rows behave predictably.
2. Gate entitlements on it: currency grants from subscription products, premium
   inventory slots, and any season/quest track unlocked by a subscription. Refuse
   with `V2Error::failed_precondition` and a stable message.
3. Add `GET /v2/iap/subscription/active` (`Authed`) returning the effective
   entitlement set.
4. Re-verify with the §"Pattern A" IAP path when a subscription is older than
   `revalidation_days` (do not call Google/Apple on every read).
5. Tests: expired row → refused; grace-period row → allowed; cancelled → refused;
   active product id → allowed; the revalidation branch against a mock store
   server.

### 4.5 Signed outbound webhooks

Facts: **no webhook code at all** (0 hits). HMAC machinery exists as precedent:
`crates/engine-api/src/infra/seats/registry.rs:9 HmacSha256` + `verify.rs:12,43`,
and `src/utils/totp.rs:69`.

Work:

1. Tables `webhook_endpoint` (`url, secret, events[], is_active, create_time`)
   and `webhook_delivery` (`endpoint_id, event_id, attempt, status, response_code,
   next_retry_at, create_time`). Operator CRUD behind `ConsoleAuthed` +
   `require_write()`; the secret is write-only (never returned after create).
2. Sign `timestamp + "." + body` with HMAC-SHA256; headers
   `X-CYG-Signature: v1=<hex>`, `X-CYG-Timestamp`, `X-CYG-Delivery`.
   Reject-replay window 300 s. Document the exact signing string in the console UI.
3. Delivery worker: outbox pattern (never send inside a request handler), bounded
   concurrency, exponential backoff with jitter, `webhook_max_attempts` (default
   6), dead-letter state on exhaustion, per-endpoint failure circuit that pauses
   an endpoint after N consecutive 5xx.
4. Events to emit: `account.created`, `account.deleted` (from §2.3),
   `iap.purchase.verified`, `subscription.state_changed`,
   `match.ended` (from §3.8), `report.created` (from §2.2),
   `quest.completed`, `marketplace.trade.settled`.
5. Tests: signature is verifiable with a known secret (golden vector), timestamp
   outside the window is refused by the receiver fixture, retry backoff sequence,
   exhausted attempts land in dead-letter, a slow endpoint cannot block the API.

### 4.6 Match results, stats and badges

1. `POST /v2/match/{id}/result` behind **server key only** (host reports outcome);
   validate participants against the §3.3 registry row before writing.
2. Upsert `player_stat` counters (4.2) and `leaderboard_record` in one
   transaction; derive badges from `player_stat` thresholds via a new
   `badge_definition` table rather than storing badge arrays denormalized.
3. Never trust a client-reported score: the only writers are the server key and
   the §3.4 event pipeline.
4. Tests: score write requires the server key, non-participant is refused,
   duplicate result is idempotent (4.1), badge derived at read time.

### 4.7 Per-player quotas

Facts: `crates/engine-api/src/infra/rate_limit/` (`mod.rs`, `bucket.rs
IpRateLimiter + MAX_BUCKETS`, `admit.rs`, `read.rs`) exists; state slot
`app/state/api_state.rs:65 limiter` built `:105-107` from
`control.rate_limit_rps`/`rate_limit_burst` (`crates/engine-config/src/control.rs:13-14`).
**The only consumer is matchmaking** (`services/matchmake/reserve.rs:22,27`), and
`app/middleware.rs` has **no tower limiter layer** at all.

Work:

1. New `player_quota` table (`user_id, scope, used, cap, window_start, period`).
2. Generic `quota::charge(state, uid, scope, amount)` used by expensive routes
   (login attempts, channel sends, storage writes, matchmaking reserves, webhook
   test posts, report creates); refuse with `V2Error::resource_exhausted` plus a
   `Retry-After` header when available.
3. Extend `IpRateLimiter` to keyed-per-player buckets, or add a second bucket
   map; apply as middleware in `src/app/middleware.rs` so it is not per-handler.
4. Caps come from `ControlConfig` defaults with per-scope overrides in a
   `[control.quotas]` table (Pattern from §3.1 for parsing).
5. Tests: over-cap refuses with the right code; window rolls over; console/admin
   exempt only when config says so; matchmaking keeps its existing behavior and
   messages.

### 4.8 Feature flags and A/B assignment

Facts: none exist. Nearest analogue is global key/value `Setting`
(`services/console/setting.rs`).

1. `feature_flag` table (`game_id, key, default_on, rollout_percent,
   sha256(salt) , active_from/to, audience_filter`) + `flag_variant`
   (`flag_key, variant, weight, payload`) and `flag_override`
   (`user_id, flag_key, variant`) for allowlists.
2. Deterministic assignment: `hash(flag_key + salt + user_id) % 100 <
   rollout_percent`; stable across requests; document the exact formula.
3. `GET /v2/flags` (`Authed`) returns the resolved set; a server-side resolver
   `flags::is_on(state, uid, key) -> bool` for gating inside other services
   (matchmaking pools, quest visibility, webhook events).
4. Operator CRUD behind `ConsoleAuthed` + `require_write()`, with `require_admin()`
   for flips above a rollout threshold.
5. Tests: same user gets the same variant; 0% and 100% edges; allowlist override
   wins; window expiry falls back to default; distribution sanity (± tolerance).

### 4.9 Notification delivery (push + email)

Facts: notifications are in-app only — `controllers/notification.rs:18 GET "",
DELETE "", DELETE /{id}`, `services/notification.rs:34,106,120`,
`repositories/notification/queries.rs:4 create_notification, :110
list_notifications_paged`, created by `services/social/notifications/mod.rs:43`,
realtime list `services/realtime/notifications.rs:23`. `push_token_{ios,android,
amazon,huawei,web}` are **stored** (`src/dto/account.rs:125-129`) and **never
used**. Zero hits for `smtp|mailer|sendgrid|ses|firebase|apns`.

Work:

1. `notification_preference` table (`user_id, channel, kind, enabled`) +
   `delivery_attempt` (`user_id, notification_id, channel, provider, status,
   error, attempts, next_retry_at`).
2. Delivery is an outbox worker (same shape as 4.5) so a request never waits on a
   vendor. Providers behind a small trait: `apns`/`fcm` stubs are **not** allowed —
   either implement with `reqwest` (rustls only, rule 4) or return an explicit
   `V2Error::unavailable` (helper already exists:
   `crates/engine-api/src/error/v2.rs unavailable(...)`), driven by config
   presence exactly like Pattern A.
3. Email: one SMTP client (explicit `OX_EMAIL_*` config, STARTTLS/implicit TLS via
   the existing rustls stack), templates as data not code, unsubscribe token in
   the footer verified with HMAC (reuse the seats signing pattern).
4. Respect `verify_time`/`disable_time` and the GDPR erase from §2.3 (deleted
   users must not receive mail).
5. Tests: preference off → no attempt; over-quota → deferred; retry backoff;
   unconfigured provider → refused with the config message, never a silent
   success; HMAC unsubscribe link round-trip.

---

## 5. PHASE 3 — production ops (Agent A4)

### 5.1 Migrations and schema versioning

Facts: DDL is **generated**, not versioned. `crates/engine-api/src/db/schema.rs:18
ddl()` pushes `define_table()` + `define_indexes()` for the 26 models listed
`:19-46`; `ensure_schema()` (`:49`) → `db/exec/raw_all.rs:7 apply()` →
`open.rs:131 exec()`; called at boot `app/api.rs:14`. Idempotent and additive-only
because somnia 0.8.1 emits `DEFINE TABLE IF NOT EXISTS` / `DEFINE INDEX IF NOT
EXISTS` (`somnia-core-0.8.1/src/types.rs:337,339,344`). Consequences to fix: **a
changed column type is never re-applied**, no `DEFINE FIELD` is emitted (all
schemaless), and there is no ordering or history.
`services/games/schema.rs:10 ensure_tenant_schema` is a no-op stub.

Work:

1. `schema_version` table (`version u64, name, applied_at, checksum`); on boot
   read max applied version, then run only the newer steps.
2. Introduce `migrations/NNN_name.surql` loaded by `include_str!` through a
   compiled-in list (a folder of files plus `mod.rs` — no runtime glob pathing),
   executed inside one `query_transaction` per step; failure aborts boot
   (fail-closed, never half-migrated).
3. Keep the generated DDL as the "base state" (it is what new databases need) and
   run migrations **after** `ensure_schema`, so both fresh and existing installs
   converge. Add a `--check-schema` doctor report to `crates/engine-config`-driven
   `cyg-gaming doctor` (`src/main.rs:79,112`) listing pending steps.
4. Encode the P0–P2 tables added by other agents as numbered migrations, so
   production rollout is ordered and reviewable.
5. Tests: boot twice on the same database is a no-op; a migration with a bad
   statement rolls back and the version row is absent; adding a model bumps the
   pinned count and still boots.

### 5.2 Backup / restore / DR

Facts: nothing exists (no `backup|restore|pitr|cron` in `scripts/`, `deploy/`,
`docs/`). Data lives in the `./data` volume (`docker-compose.yml:10`).

1. `scripts/backup.sh` — `surreal start`-compatible export via
   `surreal export` (or `--export` through HTTP) to a timestamped gzip, checksummed;
   `scripts/restore.sh` — verify checksum, refuse to restore into a non-empty
   database unless `--force`.
2. New CLI subcommands `cyg-gaming backup` / `restore` in `src/main.rs:53`
   (hand-rolled `match cmd` — follow the `schema`/`cook` arms at `:78/:87` for the
   arg style; there is **no clap** in this repo) wrapping the same logic, plus a
   cron example in `deploy/README.md`.
3. Document RPO/RTO, retention, off-box copy, and a restore drill checklist in
   `docs/` (or `architecture/`).
4. Tests: shellchecked scripts run against a throwaway `surrealkv://` dir and the
   export→drop→restore round-trip preserves rows (guard with a `#[cfg(test)]`
   skip when the `surreal` CLI is absent).

### 5.3 Observability

Facts: `/metrics` is **hand-rendered Prometheus text** —
`crates/engine-api/src/controllers/status.rs:47-97` emits 12 `# TYPE gauge` lines
(`cyg_tick_hz_est`, `cyg_players`, `cyg_ws/webrtc/udp_sessions`,
`cyg_deltas_sent`, …) by cloning one snapshot, content-type
`text/plain; version=0.0.4`, auth via `server_key_or_console` (`:56`). The metric
source is the single shared cell `ApiState.latest_status:
Arc<Mutex<Option<StatusSnapshot>>>` (`app/state/api_state.rs`), snapshot type
`crates/engine-bus/src/telemetry.rs:24` (21 wire keys incl. `tick_histogram[12]`,
p50/p99), written by `crates/engine-api/src/infra/pump.rs:7`. Logging:
`src/main.rs:32 tracing_subscriber::fmt()` + `EnvFilter`(`RUST_LOG`, default
`info`) + `.compact()`, stderr, `with_target(false)`. No JSON layer, no reload, no
OTLP/traces/spans, no Jaeger, no `prometheus`/`opentelemetry` crate.

Work:

1. Extend the hand-rendered exposition rather than importing an exporter crate
   (rule 4). Add the counters that P0–P2 create: `cyg_input_dropped_total`,
   `cyg_desync_total`, `cyg_event_dropped_total`, `cyg_webhook_delivery_total{status}`,
   `cyg_quota_refused_total{scope}`, `cyg_snapshot_save_total`, plus one
   histogram for tick duration sourced from the existing `tick_histogram`.
   Keep `/metrics` behind `server_key_or_console`.
2. Structured logs: switch the fmt layer to a JSON target when
   `OX_LOG_FORMAT=json`, keep `compact` as default, and propagate a
   `trace_id`/`request_id` field per request (new small middleware in
   `src/app/middleware.rs`). That request id doubles as the idempotency
   correlation key in §4.1.
3. `docs/` dashboards: Prometheus scrape config + a minimal Grafana/JSON panel
   set for the gauges above; alert rules for tick p99 budget, desync rate, webhook
   dead-letter growth, snapshot failures.
4. Only if the budget check passes (record the `cargo tree` + image-size delta in
   the PR description) consider the `tracing-opentelemetry` bridge with an OTLP
   exporter behind a cargo feature, off by default.
5. Tests: `/metrics` output still parses (assert the `# TYPE` lines and one new
   counter), JSON log mode emits `trace_id`, missing request id is generated.

### 5.4 Kubernetes + Agones manifests

Facts: `docker-compose.yml` runs `surrealdb/surrealdb:latest` (port 8000,
volume `./data`) + `server` built from `fedora:latest` exposing **19224**
HTTP/WS/dashboard and **7977/7778/7978** UDP, env
`OX_ALLOW_DEFAULT_SECRETS=1`, `RUST_LOG`, `GAME_DIR`, `SURREAL_*`, bind mount
`.:/app`, entrypoint `deploy/entrypoint.sh`. `deploy/Dockerfile` is
`fedora:latest`, non-root uid 10001, copies `target/release/cyg-gaming`,
`HEALTHCHECK curl http://127.0.0.1:7777/health`, `EXPOSE 7777/8090/7778-79`.
`deploy/Dockerfile.scratch` is a multi-stage musl static `scratch` image.
`deploy/docker-compose.game.yml` = single game service, no DB. `deploy/README.md`
documents an nginx TLS recipe (`:63`). **No k8s YAML, no Helm chart, no Agones
GameServer/Fleet manifests** — `deploy/README.md:125` says so explicitly, while
the Agones *runtime* integration (sidecar SDK client on `:9358`) already exists
(`crates/engine-control/src/agones/{beat,side,loud}.rs`,
`crates/engine-agones/src/{health.rs,lifecycle.rs}`).

1. `deploy/k8s/`: `namespace`, `configmap` (config.toml without secrets),
   `secret` template (never a real value), `statefulset` or `deployment` for the
   control plane, `service` + `ingress` with TLS, `pdb`, `NetworkPolicy`,
   `surrealdb` StatefulSet with a `VolumeClaimTemplate`, and `podMonitor`.
2. Agones `Fleet`/`GameServer` + `GameServerAllocation` for the host pods, wired
   to the existing `OX_AGONES` (`crates/engine-config/src/parse/mod.rs:56`) code
   path and the §3.7 drain.
3. Probes: `/health` already exists (`controllers/health.rs:13`, static
   `controllers/static_routes.rs:22`, used by the Docker healthcheck). Add
   `GET /readyz` (DB live + schema migrated + pump running) and keep `/health`
   as liveness. Use `readinessProbe`/`livenessProbe`/`startupProbe` accordingly;
   UDP ports need `hostPort`/`direct` Agones ports.
4. `deploy/helm/` optional but keep the raw manifests working without it.
5. Note for review: the compose `server` currently sets
   `OX_ALLOW_DEFAULT_SECRETS=1`. Every manifest must set it to `0`/omit it, since
   `refuse_default_secrets()` (`crates/engine-control/src/run.rs:106`) is the
   guard that blocks weak session signing/refresh/server key, seat secret, console
   password and remote DB password.

### 5.5 CI

Facts: no `.github/workflows` (untracked; `scripts/release-local.sh:52` mentions
a `release.yml` that used to exist). No `rustfmt.toml`, no `clippy.toml`;
`.cargo/config.toml` is only `[build] jobs = 8`.

1. `.github/workflows/ci.yml`: `cargo fmt --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo check --workspace --all-targets`, `cargo test --workspace` (README:345
   documents this as the test command), plus `cargo build --release` and the
   image build. Pin the Rust toolchain and cache `target/`.
2. Dashboard job: `pnpm -C dashboard build` and `pnpm -C dashboard lint`
   (oxlint; `just build-dashboard` = `cd dashboard && pnpm build`, `Justfile:77`).
3. Add Justfile recipes so agents and CI run the same thing:
   `check`, `test`, `clippy`, `fmt`, `ci`.
4. `.github/workflows/release.yml` wrapping `scripts/release-local.sh` +
   `scripts/install.sh` for artifacts; `cargo deny`/`cargo auditable` only if the
   budget and offline-cache allow.
5. Tests are the CI assertions; do not add a job that does not fail on
   `service_file_sizes.rs` or `surrealkv_budget.rs` regressions — both already
   exist and must stay green.

### 5.6 Secret rotation

Facts: env is read once at boot; no vault, no rotation. Recognized `OX_*` vars
(`crates/engine-config/src/parse/mod.rs` unless noted): `OX_SEAT_SECRET` (`:51`),
`OX_AGONES` (`:56`), `OX_BLOBS` (`:68`), `OX_DB_MEMTABLE_MIB` (`:82`),
`OX_DB_CACHE_MIB` (`:85`), `OX_DATABASE_URL` (`:95`, also `DATABASE_URL`),
`OX_SIGNING_KEY` (`:105`), `OX_REFRESH_SIGNING_KEY` (`:107`), `OX_SERVER_KEY`
(`:110`), `OX_CONSOLE_USERNAME` (`:119`), `OX_CONSOLE_PASSWORD` (`:120`),
`OX_CONSOLE_SIGNING_KEY` (`:122`), `OX_CONSOLE_TOKEN_EXPIRES_SEC` (`:132`),
`OX_IAP_APPLE_SHARED_SECRET/VERIFY_URL/SANDBOX_URL` (`:139-141`),
`OX_IAP_GOOGLE_SERVICE_ACCOUNT/PACKAGE_NAME/TOKEN_URL/API_BASE` (`:143-151`),
`OX_IAP_ALLOW_UNVERIFIED` (`:159`), `OX_SOCIAL_GOOGLE_TOKENINFO_URL` (`:164`),
`OX_SOCIAL_APPLE_CLIENT_ID/KEYS_URL` (`:167-168`),
`OX_SOCIAL_FACEBOOK_APP_ID/APP_SECRET/GRAPH_URL` (`:170-178`),
`OX_SOCIAL_STEAM_WEB_API_KEY/API_URL` (`:182-185`),
`OX_SOCIAL_GOOGLE_CLIENT_IDS` (`:193`), `OX_SOCIAL_STEAM_APP_ID` (`:196`),
`OX_SOCIAL_ALLOW_UNVERIFIED` (`:202`); plus `OX_ALLOW_DEFAULT_SECRETS`
(`engine-control/src/run.rs:104`), `OX_PLANES/SIM_CORE/UDP_ADDR/TICK_HZ/WASM_PATH/
ROOM/MAX_PLAYERS` (`engine-server/src/plane/mod.rs:32-63`), `OX_MESSAGES`
(`engine-server/src/rooms.rs:69`), `OX_DASHBOARD_DIST`
(`engine-api/src/utils/dashboard/*`), `SURREAL_ENDPOINT/NS/DB/USER/PASS`
(`parse/mod.rs:62-67`). `ConsoleUser.mfa_secret`/`mfa_recovery_codes` are b64
fields (`repositories/models/console_acl_template.rs:53-55`).

1. Accept **keyed** secrets for the signing pairs: `OX_SIGNING_KEY_ID` +
   `OX_SIGNING_KEY`, and let verification accept the previous key id for one
   token TTL window (dual-key rotation, no mass logout). Same for
   `OX_SERVER_KEY` and `OX_SEAT_SECRET` (seat HMAC
   `infra/seats/registry.rs:9`).
2. Document the runbook in `docs/`: generate new key → deploy with both → drain →
   retire old; per-secret blast radius; who may read each; k8s `Secret` versioning
   from §5.4.
3. `cyg-gaming doctor` (`src/main.rs:79,112`) gains checks: secret length/entropy
   (mirror `refuse_default_secrets` rules, `run.rs:106`), dual-key overlap
   configured, `OX_IAP_ALLOW_UNVERIFIED`/`OX_SOCIAL_ALLOW_UNVERIFIED` **set to
   anything but false ⇒ fail** with the reason (rule 3).
4. Tests: a token minted with key id A verifies during the A+B window and fails
   after B-only; doctor fails on `allow_unverified=true`.

---

## 6. PHASE 4 — developer experience (Agent A4)

Dashboard facts (`dashboard/`, React 19 + Vite 8 + TS 6): Tailwind v4
(`@tailwindcss/vite`) + shadcn/base-ui + `class-variance-authority`,
`@tanstack/react-query` 5 for state, `recharts` 3 for charts (wrapped in
`components/ui/chart.tsx`), `lucide-react`, `sonner`, `next-themes`, lint =
`oxlint`. **Routing is hand-rolled hash routing, not `auto_route`** (that is the
Rust crate): `src/lib/routes.ts` (`parseRoute`/`routeHash`, `#/g/<id>/<area>`),
`src/hooks/use-hash-route.ts`, area catalogue `src/lib/areas.ts`
(`CONSOLE_AREAS`), base path `/dashboard/`. API client `src/lib/api.ts`
(`DASHBOARD_ENDPOINTS`, bearer session token, `AbortSignal.timeout`, WS
`/ws/dashboard` with `?token=`), console network `src/lib/console/index.ts`,
hooks `use-console.ts` / `use-dashboard-api.ts` / `use-metrics-ws.ts`. Pages:
`pages/console.tsx`, `pages/login.tsx`; views in
`components/console/views/` — `games/` (list/detail/**`maps.tsx`** upload with
`oxcl|map2d|map3d|navmesh` versions), `wasm-upload/studio.tsx`,
`matches/{index,detail}.tsx`, `accounts/` (incl. `wallet-panel.tsx`,
`write-dialogs.tsx`, `detail.tsx`), `monetization.tsx`, `monetization-iap.tsx`,
`leaderboards/`, `tournaments/`, `groups/`, `notifications/`, `storage/`,
`security/`, `administration/{users,acl,audit,settings}`, `monitoring.tsx`,
`limits/cores/rooms/clients`, metrics widgets `tick-chart.tsx`,
`tick-histogram.tsx`, `stat-cards.tsx`, `transport-mix.tsx`, `bus-health.tsx`,
`workers-strip.tsx`.
**Visual rules editor, 3D map preview, live log/trace viewer: do not exist** (no
`three`/`@react-three`/`babylon` dependency; `maps.tsx` only uploads/lists).
Config versioning is partial: `games.config` JSON plus `version`+`sha256`+
`is_active` on room modules and maps
(`repositories/models/console_acl_template.rs:57,77`), with no diff/rollback UI.

### 6.1 OpenAPI served + SDK parity

Facts: generation exists but is **not served** — `crates/auto/route` uses
`poem-openapi` (`crates/auto/route/Cargo.toml:19`, swagger-ui); `openapi_json()`
and `docs.rs` `openapi_routes`/`scalar_routes` exist, and the only consumer is the
console route table `crates/engine-api/src/services/console/status.rs:41`. No
`/openapi.json` route is mounted. SDK is **hand-written, not generated**:
`sdk/server/{assemblyscript,c,cpp,csharp,lua,rust,typescript}` embedded via
`include_str!` (`src/main.rs:940-965`, command `gen-interface`);
`sdk/client/{web(cyg.js,cyg.d.ts),unity,godot,unreal,native}`; typed client
bindings are generated from `schema.ts` by `schema-codegen` (`--lang cs|gdscript|
cpp|lua|all`, `src/main.rs:252`; codegen in `crates/engine-schema/src/codegen.rs`),
with an existing `--check` parity flag (`src/main.rs:324`).

1. Mount `GET /openapi.json` and a docs page (`/console/api-docs`) using the
   existing `openapi_routes`/`scalar_routes`; keep them behind `ConsoleAuthed` if
   the spec would leak internal routes. Add the new paths to the `auto_routes`
   expected list (§0.1 rule 1).
2. `cyg-gaming openapi --out spec.json` new CLI arm; CI job runs
   `schema-codegen --check` plus a diff of the generated spec against a committed
   golden file so an unintended API-shape change fails the build.
3. Regenerate `sdk/client/web` typings from the served spec where the generator
   covers it; hand-written server SDKs gain the new event/quest/marketplace
   surface only as far as their `include_str!` sources change.
4. Tests: spec is valid JSON with every registered v2 path present (assert against
   the route table, not a hardcoded list); `--check` fails on a deliberately
   mutated binding.

### 6.2 Live logs and trace viewer

1. Console view `views/observability/logs.tsx` streaming through the existing
   dashboard WS (`/ws/dashboard`, token in query — browsers cannot set
   `Authorization` on a WS handshake, see
   `crates/engine-api/src/controllers/dashboard/ws.rs`): add a `log` frame type
   fed by a `tracing_subscriber` layer that forwards to a bounded broadcast
   channel. Never block the hot path when the buffer is full.
2. Filter by level/target/game id; correlate with the §5.3 `trace_id`/`request_id`
   field. Follow `use-metrics-ws.ts` for the client hook shape.
3. Tests: bounded buffer drops rather than blocking; the WS refuses without a
   console token (mirror the existing refusal test in
   `crates/engine-api/tests/gap_edge/dashboard_socket.rs`).

### 6.3 Player timeline

1. `views/accounts/timeline.tsx` extending the existing `accounts/detail.tsx`:
   merge `gameplay_event` (§3.4), `match_history` (§3.8), `wallet_ledger`,
   `player_inventory_item` grants, `content_report` entries, login/session events,
   subscription state (§4.4) into one ordered feed behind `ConsoleAuthed` with
   cursor pagination.
2. New aggregated read
   `GET /console/account/{id}/timeline` — one service composing the existing
   repositories rather than a new denormalized table.
3. Tests: ordering across sources, tenant scoping (reuse the `guard()` pattern
   from `controllers/dashboard/api.rs`), pagination cursor stability.

### 6.4 Economy dashboard

1. `views/economy.tsx`: sinks/sources from `wallet_ledger`, purchase volume by
   store/environment (the IAP `environment` codes 1=SANDBOX, 2=PROD, else
   "UNKNOWN" are already recorded), quest claim rate, marketplace/trade flow
   (§4.3), refund/desync pressure. Use `recharts` via `components/ui/chart.tsx`.
2. Server side: one `GET /console/economy/summary?from=&to=&game=` aggregate
   behind `ConsoleAuthed`; add the necessary SurrealQL to the existing
   `repositories/iap/queries.rs`/`console/board` style, not a new query engine.
3. Tests: date-range edges, empty window renders zeros not errors, tenant scoping.

### 6.5 3D map preview

1. Preview from existing uploaded assets only: parse `map3d`/`navmesh`/`oxcl`
   headers already handled by `views/games/maps.tsx` upload/list, and render
   geometry + navmesh + spawn volumes in an off-thread WebGL canvas.
   **Budget rule (§0.1.4):** prefer a small hand-written WebGL renderer or a
   lazily-loaded chunk over adding `three` to the main bundle; if a 3D library is
   truly required, it must be lazy-loaded and the bundle-size delta recorded in
   the PR.
2. Reuse the dashboard's existing WS to overlay live room bounds when a preview
   of a running room is requested (`GET /console/maps/{id}/preview`).
3. Tests: parser unit tests on checked-in fixture map bytes; graceful failure on
   truncated assets (no panic, an explicit "unreadable map" state).

### 6.6 Visual rules editor

1. Model game rules as data, not code: `rules_definition` table (`game_id, key,
   version, sha256, is_active, body`) where `body` is a small typed expression
   tree (`{ all: [...] }` / `{ any: [...] }` of `{ field, op, value }`) matching
   the shape already used by quest goals (§4.2) so both share one evaluator.
2. Rust evaluator in `crates/engine-api/src/services/rules/` (new folder per
   rule 5): evaluate against a `player_stat`/`SimEvent` context, unit-tested for
   every operator; no `eval`/no string interpretation.
3. Dashboard drag-and-drop builder over that schema with diff preview, dry-run
   against the §6.3 timeline of a real player, and publish = new version row
   marked `is_active` (old versions retained).
4. Tests: evaluator truth tables per operator; publish/rollback switches
   `is_active`; a rules body that is not valid is refused at write time with the
   exact field error.

### 6.7 Config versioning and diff/rollback UI

1. Add version history for `games.config` (currently a single JSON blob) using the
   same `version`+`sha256`+`is_active` shape that room modules and maps already
   use (`repositories/models/console_acl_template.rs:57,77`).
2. `GET /console/games/{id}/config/history`, `POST .../rollback/{version}`
   (`ConsoleAuthed` + `require_write()`), and every change appended to the audit
   log (`controllers/console/audit.rs`).
3. Dashboard: version list, unified diff (text diff of the serialized JSON is
   enough), rollback with confirmation; mirror `wasm-upload/studio.tsx`, which
   already shows version + sha256.
4. Tests: rollback produces a new row (never mutates history), audit entry exists,
   sha256 mismatch is refused, tenant scoping.

### 6.8 CLI additions

Facts: no clap; `src/main.rs:53` hand-rolled `match cmd`. Existing arms: `init`
(`:54`), `run` (`:60`), `aot`/`build` (`:66`), `gen-interface`/`interfaces`/`sdk`
(`:72`), `schema`/`gen-schema` (`--dump`, `--json`, `:78`/`:210`), `doctor`
(`:79`,`:112`), `schema-codegen`/`schema-gen` (`--lang`, `--out`, `--check`,
`:85`/`:252`), `schema-debug`/`schema-inspect` (`--json`, `:86`/`:370`), `cook`
(`:87`), `help`. Bins: package `cyg-gaming` + explicit `[[bin]] cyg-game`
(`Cargo.toml:63`).

1. New arms, each following the existing style (parse flags manually, print usage
   on error, nonzero exit on failure): `backup`, `restore` (§5.2), `migrate` +
   `schema-status` (§5.1), `openapi` (§6.1), `secrets audit` (§5.6),
   `config diff <a> <b> --game <id>` (§6.7), `rules check --file` (§6.6),
   `logs tail` (§6.2).
2. Move each implementation into a module under `src/cli/` (new folder;
   `src/main.rs` keeps only dispatch) so the file-size rule is respected.
3. Update `help` output, `deploy/README.md`, `docs/`, and
   `deploy/entrypoint.sh` subcommand dispatch if a command must run in-container.
4. Tests: `--check`-style dry runs exit nonzero on bad input; `schema-status`
   lists pending steps; add the `help` snapshot to the existing root tests.

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
