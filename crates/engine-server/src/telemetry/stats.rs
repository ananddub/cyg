//! Ring buffers and the rank statistics computed over them.

/// Samples kept per window: 1 s at ≤ 512 inputs/s or 128 ticks/s fits easily.
pub(super) const RING: usize = 1024;

/// Push one sample into a circular ring; once full it overwrites the oldest.
/// `n` is a free-running count — the window is `min(n, RING)` entries.
pub(super) fn ring_push(buf: &mut [u64; RING], n: &mut usize, sample: u64) {
    buf[*n % RING] = sample;
    *n = n.saturating_add(1);
}

/// The current window, sorted for percentile reads.
pub(super) fn window(buf: &[u64; RING], n: usize) -> Vec<u64> {
    let count = n.min(RING);
    let mut v = buf[..count].to_vec();
    v.sort_unstable();
    v
}

/// Nearest-rank percentile: rank = ceil(len * num / den), clamped into range.
pub(super) fn percentile(sorted: &[u64], num: usize, den: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * num + den - 1) / den;
    sorted[(rank - 1).min(sorted.len() - 1)]
}

/// Nominal period of one tick at `tick_rate_hz`, microseconds. Unknown rates
/// report 0 rather than a plausible 128 Hz period.
pub(super) fn nominal_tick_us(tick_rate_hz: u32) -> u64 {
    if tick_rate_hz == 0 {
        0
    } else {
        1_000_000 / tick_rate_hz as u64
    }
}
