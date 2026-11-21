// Verus proof harness. Run with scripts/verify-verus.sh:
//   verus --crate-type lib proofs/verify.rs
// This crate root `include!`s the exec modules (plain Rust, the SAME files
// cargo compiles for parity tests) and states the proofs about them.

#![allow(unused)]
#![cfg_attr(verus, feature(proc_macro_hygiene))]

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// seq: mathematical definition of the RFC-1982 style u16 "newer" relation.
// The exec functions in exec/seq.rs are required (via their `#[verus_spec]`)
// to match these.
// ---------------------------------------------------------------------------

/// Normalized forward distance from `last` to `tick` in wrapping u16 space,
/// in [0, 65536).
pub open spec fn s_delta(last: u16, tick: u16) -> int {
    let diff = (tick as int) - (last as int);
    if diff >= 0 { diff } else { diff + 65536 }
}

pub open spec fn s_is_newer(last: u16, tick: u16) -> bool {
    let d = s_delta(last, tick);
    d != 0 && d < 0x8000
}

spec fn s_is_superseded(newest: u16, s: u16) -> bool {
    !s_is_newer(newest, s)
}

include!("../exec/seq.rs");

/// A tick is never newer than itself.
proof fn seq_irreflexive(t: u16)
    ensures
        !s_is_newer(t, t),
{
}

/// `is_superseded` is exactly the complement of `is_newer`.
proof fn seq_superseded_complement(newest: u16, s: u16)
    ensures
        s_is_superseded(newest, s) == !s_is_newer(newest, s),
{
}

/// Antisymmetric: two counters can never both be ahead of each other.
proof fn seq_antisymmetric(a: u16, b: u16)
    requires
        s_is_newer(a, b),
    ensures
        !s_is_newer(b, a),
{
}

/// Totality for counters that are NOT exactly half the counter space apart:
/// any two u16 values not separated by exactly 32768 are ordered — exactly
/// one direction is "newer". (Distance 32768 is the RFC-1982 ambiguous zone:
/// neither direction is newer.)
proof fn seq_total_distinct(a: u16, c: u16)
    requires
        a != c,
        (a as int) - (c as int) != 32768,
        (c as int) - (a as int) != 32768,
    ensures
        s_is_newer(a, c) || s_is_newer(c, a),
{
    let diff = (c as int) - (a as int);
    let d = if diff >= 0 { diff } else { diff + 65536 };
    let e = if diff >= 0 { 65536 - diff } else { -diff };
    assert(d == (((c as int) - (a as int)) % 65536 + 65536) % 65536);
    assert(e == (((a as int) - (c as int)) % 65536 + 65536) % 65536);
    assert(e == 65536 - d);
    assert(diff >= -65535 && diff <= 65535);
    if diff >= 0 {
        assert(d == diff && e == 65536 - diff);
        if d < 0x8000 {
            assert(d != 0) by {
                assert(a != c);
            }
            assert(s_is_newer(a, c));
        } else {
            assert(d >= 0x8000 && d != 32768);
            assert(e >= 1 && e < 0x8000);
            assert(s_is_newer(c, a));
        }
    } else {
        assert(d == diff + 65536 && e == -diff);
        if d < 0x8000 {
            assert(e >= 0x8000 && e <= 65535);
            assert(s_is_newer(a, c));
        } else {
            assert(d != 32768);
            assert(e >= 1 && e < 0x8000);
            assert(s_is_newer(c, a));
        }
    }
}

/// Transitivity: if b is ahead of a and c ahead of b, then one of the two
/// directions a<->c is "newer" — provided the total advance is not exactly
/// the half-space 32768 (the ambiguous zone).
proof fn seq_transitive_within_half(a: u16, b: u16, c: u16)
    requires
        s_is_newer(a, b),
        s_is_newer(b, c),
        s_delta(a, b) + s_delta(b, c) != 32768,
    ensures
        s_is_newer(a, c) || s_is_newer(c, a),
{
    if a == c {
        assert(s_is_newer(b, a)) by {
            seq_antisymmetric(a, b);
        }
        seq_antisymmetric(b, a);
    }
    let d_ab = s_delta(a, b);
    let d_bc = s_delta(b, c);
    assert(d_ab >= 1 && d_ab <= 32767);
    assert(d_bc >= 1 && d_bc <= 32767);
    // Unfold s_delta at (a,b) and (b,c): the raw diffs differ from the
    // normalized deltas only by a single 65536 wrap.
    assert((b as int) - (a as int) == d_ab || (b as int) - (a as int) == d_ab - 65536);
    assert((c as int) - (b as int) == d_bc || (c as int) - (b as int) == d_bc - 65536);
    let d_ac = d_ab + d_bc;
    assert(d_ac >= 2 && d_ac <= 65534);
    assert(d_ac != 32768);
    // Chain the wraps: a -> b -> c crosses at most one full wrap since the
    // total advance stays under 65536.
    assert((c as int) - (a as int) == d_ac || (c as int) - (a as int) == d_ac - 65536);
    assert(s_delta(a, c) == d_ac);
    if d_ac < 0x8000 {
        assert(s_is_newer(a, c));
    } else {
        assert(s_delta(c, a) == 65536 - d_ac);
        assert(s_is_newer(c, a));
    }
}

// ---------------------------------------------------------------------------
// delta_wire: little-endian reads, frame parse bounds, fragment counts,
// and the write -> parse round-trip.
// ---------------------------------------------------------------------------

/// Mathematical definition of the little-endian u16 byte read. The exec
/// `le_u16` is required (via its `#[verus_spec]`) to match.
pub open spec fn s_le_u16(bytes: &[u8], at: usize) -> u16 {
    ((bytes[at as int] as int) + ((bytes[(at + 1) as int] as int) * 256)) as u16
}

/// Mathematical definition of the little-endian u32 byte read. The exec
/// `le_u32` is required (via its `#[verus_spec]`) to match.
pub open spec fn s_le_u32(bytes: &[u8], at: usize) -> u32 {
    ((bytes[at as int] as int)
        + ((bytes[(at + 1) as int] as int) * 256)
        + ((bytes[(at + 2) as int] as int) * 65536)
        + ((bytes[(at + 3) as int] as int) * 16777216)) as u32
}

include!("../exec/delta_wire.rs");

include!("../exec/fixed_point.rs");

// The ring invariant every `rh_*` function requires of its `h` argument:
// head < capacity, count <= capacity, parallel vecs exactly `count` long.
pub open spec fn rh_ok<T>(h: &RingHistory<T>) -> bool {
    h.head < h.capacity
        && h.count <= h.capacity
        && h.ticks.len() == h.count
        && h.data.len() == h.count
        && h.capacity >= 2
        && (h.head == h.count || h.count == h.capacity)
}

/// Physical slot for logical index `i` (0 = oldest). Mirrors the original's
/// `sample_at` index arithmetic.
pub open spec fn rh_phys<T>(h: &RingHistory<T>, i: int) -> int {
    ((h.head as int + h.capacity as int) - h.count as int + i) % (h.capacity as int)
}

/// The stored tick at logical index `i`.
pub open spec fn rh_tick_at<T>(h: &RingHistory<T>, i: int) -> u64 {
    h.ticks@[rh_phys(h, i)]
}

include!("../exec/ring_history.rs");

include!("../exec/match_outcome.rs");

// Spec-level twin of the transition table (see exec/phase.rs).
pub open spec fn s_phase_allows(self_: RoomPhase, next: RoomPhase) -> bool {
    match self_ {
        RoomPhase::Created => next == RoomPhase::Active || next == RoomPhase::Closed,
        RoomPhase::Active => next == RoomPhase::Hibernating || next == RoomPhase::Closed,
        RoomPhase::Hibernating => next == RoomPhase::Active || next == RoomPhase::Closed,
        RoomPhase::Closed => false,
    }
}

// The verified twin of `RoomPhase::allows` (kept beside the spec twin because
// the attr-mode exec mirror cannot connect its exec match to a spec match).
// Parity with the engine original is pinned by tests/verus_parity.rs.
#[verus_verify]
pub open spec fn s_room_phase_closed(p: RoomPhase) -> bool {
    p == RoomPhase::Closed
}

/// `Closed` is terminal: nothing may follow it.
proof fn phase_closed_terminal(next: RoomPhase)
    ensures
        !s_phase_allows(RoomPhase::Closed, next),
{
    assert(s_phase_allows(RoomPhase::Closed, next) == false);
}

/// No self-transition is legal in any phase.
proof fn phase_no_self_transition(p: RoomPhase)
    ensures
        !s_phase_allows(p, p),
{
    match p {
        RoomPhase::Created => {}
        RoomPhase::Active => {}
        RoomPhase::Hibernating => {}
        RoomPhase::Closed => {}
    }
    assert(s_phase_allows(p, p) == false);
}

/// A room can never return to `Created` once it has left it. Combined with
/// the two lemmas above, this fully characterizes the graph: `Closed` is
/// terminal, no phase loops to itself, and the only cycle is the legal
/// Active <-> Hibernating hibernation pair.
proof fn phase_created_is_initial(from: RoomPhase, to: RoomPhase)
    requires
        s_phase_allows(from, to),
    ensures
        to != RoomPhase::Created,
{
    match from {
        RoomPhase::Created => {}
        RoomPhase::Active => {}
        RoomPhase::Hibernating => {}
        RoomPhase::Closed => {}
    }
    assert(s_phase_allows(from, to));
}

include!("../exec/phase.rs");



/// Mathematical floor division by a positive divisor (matches Rust's
/// arithmetic shift-right semantics used by the Q32.32 code).
pub open spec fn s_floor_div(x: int, d: int) -> int
    recommends
        d != 0,
{
    if d > 0 {
        if x >= 0 {
            x / d
        } else {
            -((-x) / d + (if (-x) % d != 0 { 1int } else { 0int }))
        }
    } else {
        let ax = if x >= 0 { x } else { -x };
        let q = ax / (-d);
        let corr = if ax % (-d) != 0 { 1int } else { 0int };
        if x >= 0 { -(q + corr) } else { q }
    }
}

/// Rust (truncating) division semantics, built on Verus's Euclidean `/` and
/// `%`. This is what the engine's Rust code actually computes for `/` on
/// negative operands, where Verus's own Euclidean `/` would differ.
pub open spec fn s_trunc_div(x: int, d: int) -> int
    recommends
        d != 0,
{
    let f = s_floor_div(x, d);
    if x % d == 0 {
        f
    } else if (x < 0) != (d < 0) {
        f + 1int
    } else {
        f
    }
}



// NOTE: the write -> parse round-trip was planned as a Verus proof, but
// calling exec functions from `ensures` (spec mode) is not supported in this
// Verus build. The round-trip is instead pinned as an exec-level property
// test in tests/verus_parity.rs, over the same verified functions.

} // verus!
