# Handoff — A4 (platform-P3-P4)

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
