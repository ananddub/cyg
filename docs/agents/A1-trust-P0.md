# Handoff — A1 (trust-P0)

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
