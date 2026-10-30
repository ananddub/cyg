# Handoff — A3 (liveops-P2)

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
