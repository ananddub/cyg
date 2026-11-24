//! Verified mirrors of the engine's most correctness-critical pure logic.
//!
//! Executable code lives in `exec/` (plain Rust, single source of truth).
//! The Verus proof harness `proofs/verify.rs` `include!`s the same files and
//! verifies them. Anything here must stay byte-for-byte in sync with the
//! engine originals; `tests/verus_parity.rs` pins that parity.

#[path = "exec/seq.rs"]
pub mod seq;

#[path = "exec/delta_wire.rs"]
pub mod delta_wire;

#[path = "exec/fixed_point.rs"]
pub mod fixed_point;

#[path = "exec/ring_history.rs"]
pub mod ring_history;

#[path = "exec/match_outcome.rs"]
pub mod match_outcome;

#[path = "exec/phase.rs"]
pub mod phase;
