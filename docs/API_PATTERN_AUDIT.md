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
| ch. 21 | Pagination (cursor) | PASS | responses publish `nextPageToken` / `prevPageToken`; requests read `pageToken` (old cursor spellings remain deserialization aliases only) | pagination e2e + SDK/dashboard consumers |
| ch. 26 | Idempotency (`Idempotency-Key`) | PASS | `app/middleware.rs::idempotency` — plane-wide, claim → replay → release, status and body preserved | `tests/idempotency_plane.rs` (4 tests) |
| ch. 7, 9 | Standard methods / custom methods | PASS | colon forms (`/games/{id}:undelete`, `/games:batch-get`) are accepted by the canonical-path adapter; handlers stay ordinary Axum routes | `app/canonical_paths.rs` (2 tests) + REST `:batch-get` |
| ch. 24 | Schema-first spec | PASS | every operation publishes a concrete request/response schema; `Response` handlers name their reply through `response_body = <DTO>`, and `Json`/`ValidatedJson`/`Form`/`Body` requests infer their DTO, plain `String` bodies publish `text/plain`, multipart handlers name their upload DTO, and bodyless POSTs publish no request body | OpenAPI ratchet is empty; `requests_are_not_free_form` and `responses_name_a_concrete_dto` both pass; 510 lib tests pass |
| ch. 25 | Soft deletion / undelete | PASS | games has typed archive/restore; the book layer restores any status-bearing resource through `POST /resource/{id}:undelete` | games REST + `BookCompat::undelete` |
| ch. 27 | Request validation (`validateOnly`) | PASS | global PATCH/`updateMask`/`validateOnly` over every replaceable resource; games keeps its typed implementation | `BookCompat` merge tests + group REST |
| ch. 28 | Resource revisions | PASS | every single-resource read publishes `ETag` and `@revision` from `version`/`update_time`; stale `If-Match` is refused with 412 | group REST conditional-write test |
| ch. 22 | Filtering | PASS | uniform `?filter=` expression (`= != > >= < <= ~`, `AND`/`OR`, nested paths) applied to list replies | `app/book_filter.rs` tests + group REST |
| ch. 18 | Batch operations | PASS | games typed batch; every other collection gets `:batch-get` / `:batch-delete` with per-item results | group REST batch test |
| ch. 19 | Criteria-based deletion | PASS | games typed filter delete; every collection gets `:delete-by-filter` using the uniform filter grammar | group REST criteria-delete test |
| ch. 30 | Request authentication | PASS | Basic server-key calls are HMAC-signed with timestamp/nonce/body hash; replay and stale windows are refused | `app/book_signing.rs` tests |
| ch. 8 | Partial updates / field masks | PASS | global PATCH merges the current representation and honours `updateMask`; games keeps its typed mask | `BookCompat` merge tests + games/group REST |
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

## Global book layer

`crates/engine-api/src/app/book_compat.rs` applies the cross-resource rules in
front of the router, so a new controller inherits them:

- `PATCH` + `updateMask` + `validateOnly`
- `ETag` / `If-Match` / `@revision`
- `:batch-get`, `:batch-delete`, `:delete-by-filter`
- `:undelete` for a representation carrying `status`
- `?filter=` via `app/book_filter.rs`
- signed Basic requests via `app/book_signing.rs` (`CYG_SIGNING=off` only for
  local/test runs; the deployed default is required)
