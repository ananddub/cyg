// Verified mirrors of the room phase transition table and the host tick
// catch-up clamp.
//
// Keep in sync with:
// - crates/engine-bus/src/telemetry/mod.rs (`RoomPhase::allows`)
// - crates/engine-server/src/host/tick_gate.rs (catch-up clamp in `run`)

/// Mirror of `engine_bus::RoomPhase` (order and meaning identical).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus, verus_verify)]
pub enum RoomPhase {
    Created,
    Active,
    Hibernating,
    Closed,
}

/// Mirror of `RoomPhase::allows`: the only legal transitions.
///
/// The verified twin lives in proofs/verify.rs (`s_phase_allows` plus proof
/// lemmas: `Closed` is terminal, no self-transition is legal, the only cycle
/// is Active <-> Hibernating, and `Created` is never re-entered). Behavior
/// parity with the engine original is pinned by tests/verus_parity.rs.
#[cfg_attr(verus, verus_verify)]
pub fn phase_allows(self_: RoomPhase, next: RoomPhase) -> bool {
    match self_ {
        RoomPhase::Created => next == RoomPhase::Active || next == RoomPhase::Closed,
        RoomPhase::Active => next == RoomPhase::Hibernating || next == RoomPhase::Closed,
        RoomPhase::Hibernating => next == RoomPhase::Active || next == RoomPhase::Closed,
        RoomPhase::Closed => false,
    }
}

/// Mirror of the catch-up clamp from `TickRunner::run`: given wall-clock
/// elapsed ticks and the watchdog budget, decide how many ticks to run and
/// what the next `last_tick` anchor is.
///
/// The engine original clamps `ticks_to_run` to `max_ticks_per_frame`; when
/// clamped it RESETS the anchor to "now" (dropping the backlog) instead of
/// advancing it, so a stalled server never spirals into an unpayable debt.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        elapsed_ticks >= 1int,
        max_ticks_per_frame >= 1int,
        elapsed_ticks < 4611686018427387907int,
        max_ticks_per_frame < 4611686018427387907int,
        last_tick_ms >= 0int,
        interval_ms >= 1int,
        now_ms >= last_tick_ms,
        last_tick_ms + elapsed_ticks * interval_ms < 9223372036854775807int,
        last_tick_ms + elapsed_ticks * interval_ms > -9223372036854775808int,
    ensures
        r.ticks_to_run >= 1int && r.ticks_to_run <= max_ticks_per_frame,
        r.clamped == (elapsed_ticks > max_ticks_per_frame),
        r.clamped ==> r.next_last_tick_ms == now_ms,
        !r.clamped ==>
            r.next_last_tick_ms == last_tick_ms + r.ticks_to_run * interval_ms,
))]
pub fn tick_clamp(
    elapsed_ticks: i64,
    max_ticks_per_frame: i64,
    last_tick_ms: i64,
    interval_ms: i64,
    now_ms: i64,
) -> TickClamp {
    let ticks_to_run = if elapsed_ticks > max_ticks_per_frame {
        max_ticks_per_frame
    } else {
        elapsed_ticks
    };
    let clamped = elapsed_ticks > max_ticks_per_frame;
    let next_last_tick_ms = if clamped {
        now_ms
    } else {
        last_tick_ms + ticks_to_run * interval_ms
    };
    TickClamp {
        ticks_to_run,
        clamped,
        next_last_tick_ms,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus, verus_verify)]
pub struct TickClamp {
    /// How many ticks to simulate this frame (never exceeds the budget).
    pub ticks_to_run: i64,
    /// True when the watchdog budget bit and the backlog was dropped.
    pub clamped: bool,
    /// The next `last_tick` anchor: reset to now when clamped, otherwise
    /// advanced by exactly the ticks simulated.
    pub next_last_tick_ms: i64,
}
