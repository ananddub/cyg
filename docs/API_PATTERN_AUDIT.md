# API & Architecture — pattern audit

Living checklist, same discipline as `docs/NETCODE_BOOK_AUDIT.md`: every claim is
mapped to a code path and to a test, or to a known gap with the phase that closes
it. Reference root: `/mnt/SSD/Books/System Design & Architecture/`.

A second pass (round two, `docs/API_PATTERN_PLAN.md`) closed the plane-level
gaps these rows had left open: `engine-api` is now a CI gate, duplicate route
mounts are gone, `/v1` is the only base (`/v2` answers 404), and the
`&ApiState` god-parameter has a machine-checked
ratchet with a first converted domain.

Status: **PASS** = implemented and tested, **PARTIAL** = implemented with a known
limitation, **GAP** = not implemented. The `Phase` column names the workstream in
`docs/API_PATTERN_PLAN.md`, which records what each phase delivered and what it
deliberately left for later.

## Layering and structure

| Reference | Pattern | Status | Code | Evidence |
|---|---|---|---|---|
| Fowler, *PoEAA* | Layered architecture (presentation / service / data source) | PASS | `crates/engine-api/src/{controllers,services,repositories,dto,error}` | no `sqlx`/query text in `controllers/` or `services/` |
| Fowler, *PoEAA* | Service Layer | PASS | `services/` (271 files) | controllers hold no business rules |
| Fowler, *PoEAA* | Repository | PASS | `repositories/` (114 files), single `Datastore` handle | `db/exec.rs` owns the only connection |
| Fowler, *PoEAA* | DTO / model separation | PASS | `dto/` (54 files) vs `repositories/models/` | wire shape never a DB row |
| Fowler, *PoEAA* | Unit of Work | PARTIAL | hand-written multi-statement SurrealQL beside services (12 sites) | no transaction abstraction; documented convention in README |
| Fowler, *PoEAA* | **Service Locator** (anti-pattern) | PARTIAL | `app/state/installed.rs` `current_state()` + `resolve::<T>()`; `deps::q` prefers the DI container and falls back to the state bag | Phase 6 pins the 5 domains that use it (`tests/layering.rs`) |
| Martin, *Clean Architecture* | Dependency rule (inner layers know nothing of outer) | PARTIAL | services return `Json<T>` in places (`services/leaderboard/list.rs`) | Phase 2 |
| — | No god objects | PASS | services inject `Arc<Plane>` / `Arc<Store>`; `&ApiState` is a parameter of **0** service methods | `tests/layering.rs::the_state_parameter_only_shrinks` pins the count at 0; `tests/api_state_param_baseline.txt` |

## API surface (Geewax, *API Design Patterns*)

| Reference | Pattern | Status | Code | Evidence |
|---|---|---|---|---|
| ch. 24 | Versioning in the path | PASS | `#[controller("/v1/...")]` (302 declarations) | `/v1` retired |
| — | Standard error model | PASS | `error/v2.rs` 16 canonical codes; `error/api.rs` derives its code from that table, one source of truth | `error::tests::the_legacy_envelope_uses_the_canonical_code_for_every_status` |
| ch. 21 | Pagination (cursor) | PARTIAL | `dto/` `limit` (21), `cursor` (18) | response field naming is documented as a coordinated change in `API_PATTERN_PLAN.md` |
| ch. 26 | Idempotency (`Idempotency-Key`) | PASS | `app/middleware.rs::idempotency` — plane-wide, claim → replay → release, status and body preserved | `tests/idempotency_plane.rs` (4 tests) |
| ch. 7, 9 | Standard methods / custom methods | PARTIAL | `get`/`post`/`put`/`delete` + verb routes (`/define`, `/claim`) | — |
| ch. 24 | Schema-first spec | PARTIAL | `auto_route` infers from `Result<Json<T>, E>`; DTOs need `poem_openapi::Object`; leaderboard domain converted as the reference | ratchet: `src/tests/openapi.rs` + `tests/openapi_undocumented.txt` (297 routes still to convert) |
| ch. 25 | Soft deletion / undelete | PARTIAL | games archive keeps the row; `POST /v1/console/games/{gameId}:undelete` restores it | `restore_game` tests (service + REST) |
| ch. 27 | Request validation (`validateOnly`) | PARTIAL | games `PATCH`/`PUT` accept `validateOnly`; other dangerous domains still need the same flag | `validate_only_returns_the_planned_row_and_writes_nothing` |
| ch. 28 | Resource revisions | PARTIAL | game config history is a domain read; the book's `@revision` read is not universal | `config_history`, `config_version` |
| ch. 22 | Filtering | PARTIAL | domain-specific query parameters; no uniform filter string | — |
| ch. 18 | Batch operations | GAP | no atomic batch create/update/delete surface | — |
| ch. 19 | Criteria-based deletion | GAP | no filter-driven bulk delete | — |
| ch. 30 | Request authentication | PARTIAL | bearer/session auth over TLS; no signed-request canonicalization | `app/middleware.rs` |
| ch. 8 | Partial updates / field masks | PARTIAL | `PATCH /v1/console/games/{gameId}` + `updateMask`; `PUT` remains the whole-resource compatibility path | `services/games/service/update_game.rs::masked_update` (4 unit tests) + `tests/rest_e2e/games.rs` |
| ch. 28 | Optimistic concurrency (ETag / If-Match) | PASS | `utils/precondition.rs`; ACL templates carry a `version` generation and a guarded update | `tests/precondition_plane.rs` (3 tests) |
| ch. 10 | Long-running operations | N/A | no operation is slow enough to need a job handle | — |

## Reliability (Nygard, *Release It!*)

| Pattern | Status | Code | Evidence |
|---|---|---|---|
| Timeouts on every outbound call | PASS | `iap/verify`, `auth/verify`, `webhook/worker`, `infra/notify` | builder timeout in each |
| Retries with backoff + jitter | PASS | `services/webhook/deliver.rs::retry_delay` | strictly increasing, capped, jittered |
| Circuit breaker (webhook endpoints) | PASS | `services/webhook/worker.rs::Circuit` | trips after N failures, cooldown |
| Circuit breaker (other outbound) | PASS | `infra/http` — per-destination breaker + bulkhead; wired into IAP, OAuth, APNs, FCM | `infra::http::tests` (4 tests) |
| Bulkhead / concurrency isolation | PASS | webhook + notify workers cap in-flight work; `infra/http` caps concurrent calls per destination | `infra::http::tests::a_dead_destination_stops_being_asked` |
| Rate limiting / quotas | PASS | `services/quota`, `app/middleware.rs::{ip,player}_rate_limit` | per-address and per-account windows |
| Load shedding / readiness gate | PASS | `middleware::lifecycle_gate` | 503 + `Retry-After` while booting/draining |
| Graceful shutdown | PASS | `app/serve.rs` `with_graceful_shutdown` | — |
| Feature flags | PASS | `services/flag/{assign,resolve}` | — |
| Bulkheads for the simulation | PASS | `engine-bus` bounded channels, `engine-manager` thread topology | — |

## Distributed systems (Burns; Kleppmann; Vitillo)

| Pattern | Status | Code | Evidence |
|---|---|---|---|
| Transactional outbox | PASS | `repositories/models/webhook.rs` delivery rows | request enqueues, worker drains |
| Dead letter | PASS | `repositories/webhook::{DEAD,DELIVERED}` | abandoned rows kept as evidence |
| At-least-once + dedupe by `event_id` | PASS | `services/webhook/deliver.rs::envelope` | receiver can dedupe |
| Retry-safe writes | PASS | `app/middleware.rs::idempotency` | every `POST`/`PUT`/`PATCH`/`DELETE` that carries the header |
| Multi-tenancy isolation | PASS | `services/games/tenant.rs`, `game_id` filters | console refuses other tenants |
| Idempotent consumer / replay guard | PASS | `infra/events.rs` cursors | — |

## Testing

| Pattern | Status | Code | Evidence |
|---|---|---|---|
| Sociable integration tests over the real router + store | PASS | `tests/server.rs` spins an HTTP server | 513 test files |
| Contract tests for error codes | PASS | `tests/Reply::refusal` asserts code+status | — |
| OpenAPI response coverage guard | PASS | `src/tests/openapi.rs` | fails on a new untyped handler and on a converted one left in the baseline |
| Layering guard | PASS | `tests/layering.rs` | no query text in services, no `current_state()` in services, service-locator domains pinned |
