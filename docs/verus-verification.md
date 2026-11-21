# Verus verification for the engine's critical logic

Formal proofs (AWS's [Verus](https://verus-lang.org)) over the engine's most
correctness-sensitive pure logic. This is **hard checking, not testing**: the
properties hold for *all* inputs, not just the ones a test tried.

## How it works

Verus cannot verify crates that depend on bevy/rapier/wasmtime/tokio, so the
critical logic lives twice — once in the engine, once as a verified mirror:

```
verus/ox-verified/src/exec/*.rs     plain Rust, single source of truth for the mirror
verus/ox-verified/src/proofs/verify.rs   Verus harness: includes exec/ and states the proofs
tests/verus_parity.rs               behavior parity: mirror vs engine originals
scripts/verify-verus.sh             runs the proofs
```

- `exec/` files are plain Rust compiled by normal `cargo` (used by the parity
  tests). The same files are `include!`d into `proofs/verify.rs`, which is
  verified with the Verus toolchain (`--cfg verus` activates the spec
  attributes).
- Anything in `exec/` must stay byte-for-byte in sync with the engine
  original it mirrors (each file names its original at the top). The parity
  tests pin the behavior; the header comments pin the provenance.

## What is verified (status: 51 verified, 0 errors)

| Module | Mirrors | Verified properties |
|---|---|---|
| `exec/seq.rs` | `engine-webrtc-protocol` `is_newer`/`is_superseded` (verbatim) | RFC-1982 wraparound: irreflexive, antisymmetric, total except the exact-half-space edge (32768 — the RFC's ambiguous zone, proven real by the proof attempt), transitive outside it |
| `exec/delta_wire.rs` | `channel/delta.rs` + byte helpers | `DeltaHeader` parse never reads OOB; `frag_idx < frag_total`, chunk ≤ 1191, frame ≤ 1200 on success; `delta_fragment_count` in `1..=255` iff body fits; `write_delta_frame` exact wire layout (header byte facts proven through the chunk-copy loop) |
| `exec/fixed_point.rs` | `engine-physics/fixed` Q32.32 | add/sub/neg/abs: no-panic contracts; mul: full i128 product, floor-divide by 2^32, **no silent truncation in the proven domain**; div: saturating-by-zero semantics + Rust-truncating quotient (computed via magnitudes because Verus's `/` is Euclidean); clamp: result within `[min, max]` |
| `exec/ring_history.rs` | `engine-core/rewind/history.rs` ring core | head/count/capacity coherence, pre-wrap vs wrapped dichotomy (`head == count` or `count == capacity`), stale-record rejection, logical→physical index bounds |
| `exec/match_outcome.rs` | `engine-server` match-result decoder | the full sequential byte-walk never reads OOB; success implies `1 <= participants <= 64` and full consumption; same rejection rules (version, count gates, metadata cap, stat-name 1..=64, trailing bytes) |
| `exec/phase.rs` | `engine-bus` `RoomPhase::allows` + `engine-server` tick clamp | `Closed` terminal, no self-transitions, `Created` never re-entered (only cycle is the legal Active<->Hibernating pair); clamp result `1 <= ticks <= budget`, backlog-drop resets anchor, non-clamped advance is exact |

### Findings the proofs produced

- **fp_mul/fp_div silently truncate** to i64 when the Q32.32 result overflows
  (the original does this unconditionally). The verified contract documents
  the no-truncation domain; outside it the mirror behaves bit-identically.
- **The u16 "newer" relation is genuinely ambiguous at exactly 32768** — the
  first transitivity proof attempt found the counterexample. This matches the
  RFC-1982 ambiguity; the engine's delta/ack logic inherits it.
- **Verus's `/` is Euclidean, Rust's is truncating** — the mirrors compute
  Rust semantics explicitly (magnitude path) so proofs are about the code the
  engine actually runs.

### Out of scope for this pass

- f64 interpolation (`find_samples`/`find_bounding`) — float proofs are a
  separate project; the ring structure around it is verified.
- UTF-8 validation in `match_outcome::decode` (the mirror validates
  structure only) and the String allocation.
- `FP::sqrt` (f64 seed + u128 fixup loops).
- Concurrency (rtc_plane registries, tokio workers) — phase 2 candidate.

## Running

```bash
./scripts/verify-verus.sh          # proofs: "verification results:: N verified, 0 errors"
cargo test --test verus_parity     # mirror-vs-engine behavior parity
```

## Adding a new verified target

1. Copy the engine logic into `verus/ox-verified/src/exec/<name>.rs` with a
   header naming the original. Pure, dependency-free code only.
2. Annotate with `#[cfg_attr(verus, verus_verify)]` (+ `#[cfg_attr(verus,
   verus_spec(...))]` for contracts). Remember: impl methods cannot carry
   `verus_spec` in this build — use free functions and delegate.
3. Include it from `proofs/verify.rs`, state the spec fns there (attr-mode
   specs cannot use `verus!`-only syntax like `int`, `@`, `final(h)` as
   call args, or numeric `int` literals).
4. Add parity tests in `tests/verus_parity.rs`.
5. `./scripts/verify-verus.sh` must report 0 errors.
