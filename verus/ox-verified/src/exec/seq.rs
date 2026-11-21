// Verified mirror of `crates/engine-webrtc-protocol/src/frames/mod.rs::is_newer`
// and `crates/engine-webrtc-protocol/src/channel/mod.rs::is_superseded`.
//
// Keep this file verbatim-in-sync with those originals. Parity is pinned by
// tests/verus_parity.rs and formally proven in proofs/verify.rs (seq::).
//
// Semantics: u16 tick/seq counters wrap at 65536. `tick` is "newer" than
// `last` when it is ahead by a nonzero amount smaller than half the counter
// space (0x8000), i.e. the RFC-1982 serial-number arithmetic rule.

/// True when `tick` is strictly ahead of `last` in wrapping u16 order.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(
    ensures
        returns s_is_newer(last, tick)
))]
pub fn is_newer(last: u16, tick: u16) -> bool {
    let delta = tick.wrapping_sub(last);
    delta != 0 && delta < 0x8000
}

/// True when `seq` is not ahead of `newest` (stale, duplicate, or equal).
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(
    ensures
        returns !s_is_newer(newest, seq)
))]
pub fn is_superseded(newest: u16, seq: u16) -> bool {
    !is_newer(newest, seq)
}
