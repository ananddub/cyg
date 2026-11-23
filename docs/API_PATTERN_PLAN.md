# API & architecture — pattern plan

Companion to `docs/API_PATTERN_AUDIT.md`. This file records what each phase was
for, what landed, and what was deliberately left for later. The audit file says
what is true today; this one says why.

Reference root: `/mnt/SSD/Books/System Design & Architecture/`.

## Phase 0 — ground truth

`docs/API_PATTERN_AUDIT.md`. One row per pattern, a code path and a test for
every PASS, a named phase for every GAP. Written first so the rest of the work
had an acceptance criterion instead of an opinion.

## Phase 1 — idempotency as a plane concern

**Was:** `services/idempotency.rs` existed but five controllers called it by
hand; every other mutating route could double-write on a retry.

**Now:** `app/middleware.rs::idempotency`, layered after `player_rate_limit`.

- Claims `Idempotency-Key` before the handler runs, keyed by
  `actor + "METHOD /matched/path" + sha256(body)`.
- Stores the reply as an envelope `{__idempotency, status, raw}` so a replay is
  byte-identical and keeps the original status. `Idempotency-Replayed: true`
  tells the client which reply it received.
- Only a success is stored. A refusal releases the key, because the world it was
  refused against can change (a board created, an account topped up).
- Handlers that mint their own row id read `IdempotentUuid` from the request
  extensions, so a retry lands on the same primary key.
- Exempt: any `multipart/form-data` body (the wasm and map upload mounts take
  the file at the route root as well as at `/upload`), socket upgrades, the
  dashboard stream, `/auto`, and any path ending in `/upload`, `/wasm` or
  `/maps`. Bodies over `CYG_IDEMPOTENCY_MAX_BODY_BYTES` (1 MiB) pass through.

**Tests:** `tests/idempotency_plane.rs` — replay is byte-identical and writes
once; one key cannot describe two bodies (400 `INVALID_ARGUMENT`); a refusal does
not freeze its key; a server-key grant is keyed; a board lifecycle round-trips.
The pre-existing `tests/gap_economy` suite still passes through the middleware.

**Removed:** the hand-rolled claims in `inventory`, `iap`, `match_result`,
`console/account`, `console/notification`.

## Phase 2 — typed replies and a spec that describes them

**Was:** 172 handlers returned a type-erased `Response`, so the generated spec
documented requests but no responses.

**Now:** the reference recipe, proven on the leaderboard domain:

1. Give the DTO a `poem_openapi::Object` derive (it implements the `Type` the
   descriptor needs; `serde_json::Value` fields are supported).
2. Have the service return the DTO, not `serde_json::Value`.
3. Declare the handler as `Result<Json<T>, V2Error>`; `auto_route` infers the
   schema from the signature.

`dto/leaderboard.rs` gained `BoardDefinition`, `BoardListResponse`,
`DeletedBoard`; `services/leaderboard/define.rs` returns them; all eight
leaderboard handlers are typed.

**Ratchet:** `src/tests/openapi.rs` compares every registered route against
`tests/openapi_undocumented.txt`. A new untyped handler fails the build; a
converted handler fails until its line is deleted. Regenerate with
`CYG_DUMP_OPENAPI=1 cargo test -p engine-api --lib openapi::undocumentation`.
The remaining 297 routes are the visible backlog, not hidden debt.

**Bug found and fixed:** `GET /v1/leaderboard` answered 500 because it asked the
store for `LIMIT i64::MAX`. It now asks for 10 000.

## Phase 3 — optimistic concurrency

**Was:** every console edit was last-writer-wins.

**Now:** `utils/precondition.rs` plus a generation counter on `console_acl_template`.

- `ETag: "<version>"` names the generation on the reply.
- `If-Match: "<version>"` makes the write conditional. A stale generation is
  refused with `ABORTED` (409) naming the version that won.
- The check and the bump happen in one statement:
  `... SET version = $next WHERE version = $expected OR (version IS NONE AND $expected = 1)`.
- A request without `If-Match` keeps the unconditional write, so existing
  dashboard clients are unaffected, but it still advances the generation.

**Tests:** `tests/precondition_plane.rs` — an edit lands only on the generation
it named; a stale tag is refused with the winning version in the message; a
malformed tag is `INVALID_ARGUMENT`; an unconditional write still bumps.

**Follow-up landed:** `PATCH /v1/console/games/{gameId}` with an explicit
`updateMask` (and an implicit mask when the query is absent), plus the book's
`validateOnly` flag. `PUT` stays as the whole-resource compatibility path. The
same pattern is the reference for the remaining console domains; the audit keeps
them `PARTIAL` until they are converted.

## Phase 4 — outbound HTTP stability

**Was:** four independent `reqwest::Client` factories (IAP, OAuth, APNs, FCM)
with timeouts and no isolation. A dead vendor cost one timeout per request.

**Now:** `infra/http`.

- Circuit breaker per destination: `CYG_HTTP_BREAKER_TRIPS` consecutive
  failures (5) open it for `CYG_HTTP_BREAKER_COOLDOWN_MS` (30 s), doubling to
  `CYG_HTTP_BREAKER_MAX_COOLDOWN_MS` (5 min). Half-open lets exactly one probe
  out. Open means no socket is opened at all.
- Bulkhead per destination: `CYG_HTTP_BULKHEAD` (32) concurrent calls; the rest
  are refused immediately with `RESOURCE_EXHAUSTED` and `Retry-After`. The
  bulkhead is checked before the breaker so our own overload can never consume
  the half-open probe.
- A 4xx counts as alive; only transport failures, 5xx and 429 count against it.
- `/metrics` reports `cyg_http_breaker_state{destination}` and
  `cyg_http_breaker_failures{destination}`.

The webhook outbox keeps its own per-endpoint circuit (`services/webhook/worker.rs`),
which is a domain-level breaker with a database-backed cooldown; it is not
duplicated or replaced.

**Tests:** `infra/http/tests.rs` counts connections on a real socket — an open
breaker opens no socket, a 400 never trips it, a half-open breaker sends exactly
one probe, and a recovered destination closes.

## Phase 5 — consistency

**Done — one error table.** `error/api.rs` no longer keeps its own
status→code map; it asks the canonical kind table (`error/v2.rs`). The legacy
envelope (used only by the Nakama-compatible `/matchmake` and `/queue` routes)
therefore cannot drift from it. Test:
`error::tests::the_legacy_envelope_uses_the_canonical_code_for_every_status`.

**Deliberately deferred — pagination field naming.** Requests are already uniform
(`limit` + `cursor`). Responses are not: 18 DTOs emit `cursor`, 6 emit
`next_cursor`, 3 use `offset`. Renaming is a breaking wire change that has to
land together with every SDK under `sdk/client/` (`core-ts`, `web`, `unity`,
`godot`, `unreal`). Doing it in this pass would have shipped a half-migrated
surface to five clients. The target shape is:

```
request:  ?limit=N&cursor=X
response: { items: [...], next_cursor: "..." }   // cursor kept as an alias for one minor
```

**Landed — custom-method paths.** `app/canonical_paths.rs` rewrites the book's
`/resource/{id}:verb` spelling into the nested handler before routing, with an
allowlist of real verbs so a storage key containing `:` is untouched. Games
`undelete`, batch and criteria routes are the reference; the adapter covers the
existing custom verbs without moving every handler declaration.

**Still open — response schemas.** `/define`, `/claim`, `/kick`
should read `:define`, `:claim`, `:kick` per *API Design Patterns* ch. 11. The
route macro currently folds `:segment` into a path parameter
(`route_macros/src/docs.rs`), so this needs an explicit `is_custom_method` flag
in `OpenApiRouteDescriptor` first. Cosmetic; scheduled after the response-schema
backlog.

## Phase 6 — layering

**Already true, now enforced.** `tests/layering.rs` fails the build if:

- a service carries query text (moved to `repositories/`),
- a service reads `current_state()` (the ambient request state),
- the set of service domains using `deps::q` / `deps::blobs` changes. Today:
  `account`, `inventory`, `leaderboard`, `quota`, `realtime`.

`deps::q` is not a plain service locator: it asks the request's DI container
first (`app/state/installed.rs::resolve`) and only then the state bag. That is why
it stays as the sanctioned accessor while services still take `&ApiState`.

**Known, not yet changed.** 143 of 271 service modules import `axum` — mostly
`Json<T>` and `HeaderMap` in signatures. Moving the transport types out of the
service layer is a mechanical refactor per domain; it is safe only once each
domain's response DTOs are typed (phase 2), because the DTO becomes the return
type in both places. Doing it before that trades one leak for another.

## Order and gates

Phases were executed 0 → 1 → 2 → 3 → 4 → 5 → 6. Phase 4 is independent and can
land on its own; phase 3 needs 2; phase 5.1 (pagination) needs 2 finished.

Gate after every phase:

```bash
just fmt
just clippy
RUST_MIN_STACK=16777216 cargo test -p engine-api --lib
RUST_MIN_STACK=16777216 cargo test -p engine-api --test idempotency_plane \
    --test precondition_plane --test layering
```

### Pre-existing failures, not caused by this work

Closed in round two (`A`, above): the `gap_player` 429 came from
`AUTH_BURST = 10` against 21 device authentications from one address, and the
test harness now raises that limit for itself instead of production lowering its
guard.

---

# Round two — the plane, not the pattern

The audit above closed the pattern gaps. This round closes the *plane*: one
version, one mount per route, a CI that runs the API plane, and a delivery
pipeline that publishes on a tag.

## A — the API plane is in CI, and it is green

`engine-api` was not in CI at all: `CORE_PACKAGES` excluded it, so 486 unit
tests and 20-odd integration suites ran only on a laptop. Turning the gate on
found four real failures, all of them pre-existing:

| Failure | Cause | Fix |
| --- | --- | --- |
| `gap_player`, `rest_e2e` → 429 | 21 device auths from one address against `AUTH_BURST = 10` | `ControlConfig::auth_rate_limit_burst`, raised **only** by the test harness (`tests/harness/harness.rs`, `tests/rest_e2e/harness.rs`). Production keeps 10. |
| index queries returned nothing | `index_query` compared the text column `user_id` against `<uuid>$owner` | compare as text, like `storage_objects` always did. Regression test: `storage::tests::an_equality_term_matches_a_numeric_value` |
| `rest_e2e` "read under the nil owner" | the test expected 400; every read entry point documents nil = "mine" | test asserts the documented behaviour |
| `/cyg.js` was not the browser SDK | the checked-in bundle carried no banner the two SDK suites assert on | banner added to `sdk/client/web/cyg.js` **and** `core-ts/build.mjs`, so a rebuild keeps it |

Also: the bank of 11 service files over the 200-line budget became
`tests/service_file_size_baseline.txt` — a ratchet, like the OpenAPI backlog, so
the number can only go down.

## B — the duplicates are gone

Each of these was a second mount of the same handler:

- `/api/matchmake/{method}/{room}` → `/matchmake/{method}/{room}`
- `/api/queue/*` → `/queue/*`
- `/api/lobby/rooms` → `/lobby/rooms`
- `/api/rooms` → `/lobby/rooms`

The SDKs that named the alias (Unity `CygClient.cs`, Godot `cyg_api.gd`,
Unreal `CYGSettings.cpp`) now name the surviving path, and a test asserts the
retired alias answers 404 rather than silently working.

## C — one version, and it is `/v1`

The version literal lives in exactly one place, `app/api.rs::API_BASE`. There is
no second base and no alias.

- `#[controller("/v1/...")]` — 61 attributes, authored at the version they serve.
- `route_macros` strips that leading segment when it registers a route and marks
  the descriptor `versioned`, so the tree is mounted from one place instead of
  from 61 attributes.
- `app/api.rs` builds the versioned tree once and nests it at `API_BASE`.
- `openapi_spec()` publishes `/v1` only; `set_api_base` runs at boot, before
  anything renders a spec, so the document cannot disagree with the mount.
- Operational paths stay outside the version on purpose: `/status`, `/metrics`,
  `/readyz`, `/ws/*`, `/dashboard/*`, `/api/dashboard/*`, `/lobby`, `/queue`,
  `/matchmake`, `/openapi.json`, `/cyg.js`, `/auto/*` and the vendored SDK
  assets. They describe the process, not the game API.

**Enforced:** `tests/auto_routes/only_v1_is_versioned.rs`. Every documented path
is either under `/v1` or in that file's operational list; anything else fails.
It also probes `/v2/*` at runtime and asserts 404.

The macro no longer recognises `v2` as a version. A controller still written
`#[controller("/v2/...")]` is therefore *not* quietly rewritten: it registers a
literal `/v2` route and the guard fails on it.

## D — the god parameter is gone

`&ApiState` is a bag of everything the process owns: a service that takes it
cannot be constructed without a request, and nothing in its signature says what
it reads. It was a parameter of **388** service methods.

**What replaced it.** Two injectable handles, both `Arc`, both registered once in
`app/api.rs::register_plane`:

- `services/store.rs::Store` — the data layer as one handle
  (`Queries`, `BlobStore`, `HookRunner`). It stays optional internally: a process
  with no database still serves `/status`, `/matchmake` and the dashboard, so
  registering `Queries` as a hard instance would stop whole controllers from
  mounting.
- `services/plane.rs::Plane` — everything else a service may read: config, bus,
  seats, limiter, signals, statics, realtime, sessions, chain, port, the boot
  status channel, dev mode.

A service now takes what it uses in its constructor:

```rust
#[singleton]
impl LeaderboardService {
    fn new(plane: Arc<Plane>) -> Self { Self { plane } }
}
```

Controllers no longer thread the request bag into domain call sites; they hand
the plane to the service they build. Free functions in the service layer take
`&Plane` instead of `&ApiState`, and `deps::q` / `deps::blobs` take a plane.

**Result:** 388 → 0. `tests/api_state_param_baseline.txt` is `0`, and
`tests/layering.rs::the_state_parameter_only_shrinks` fails on any regrowth, so
the parameter cannot come back one call site at a time. It also
pins the service-locator set — which domains still reach through `deps::q` —
which is now just `realtime`, and pins the single service that still reads
`current_state()` (the lazily-built presence hub, which has no container to
resolve from).

`Plane::for_tests()` keeps unit tests that exercise store-free logic honest
without a database or a port.

**One ordering rule this introduces:** a plane is captured at construction, so a
fixture that mutates the state bag must finish before it builds the service. The
two tests that did it the other way were reordered, and the reason is written
where it bites.

## E — CI and CD

`.github/workflows/ci.yml`: `core`, `api` (lib + every integration suite),
`check`, `lint` (rustfmt **blocking**, clippy blocking on errors), `dashboard`
(lint + build), `sdk` (typecheck + test + build), `api-surface` (four ratchets),
`image` (container builds).

- `cargo fmt --all` is clean across the workspace; that is what made the gate
  possible, and it was its own change.
- `just clippy` used to be `cargo clippy -- -D warnings`, which could never pass
  with the current warning backlog, so `just ci` was permanently red. The gate is
  now "no clippy *error*"; `just clippy-strict` is the aspiration.
- `just api-test` runs the plane's own suites; `just ci` runs fmt, clippy, the
  workspace tests, the API plane and the TS core.

`.github/workflows/release.yml` (tag `v*`): tests first, then
`cyg-<triple>.tar.gz` for linux x64/arm64, macOS arm64 and Windows x64 attached
to the GitHub release, plus `ghcr.io/<repo>:<tag>` and `:latest` as a
multi-architecture image (`Dockerfile`, three stages: console → rust → runtime).

`.cargo/config.toml` asks for `target-cpu=native` and `mold`, which is right for
this machine and wrong for a distribution: both workflows set `RUSTFLAGS`, which
overrides it, and the Dockerfile builds `target-cpu=generic` so one image runs on
every host of its architecture.
