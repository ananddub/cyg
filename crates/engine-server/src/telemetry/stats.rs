
pub(super) const RING: usize = 1024;

pub(super) fn ring_push(buf: &mut [u64; RING], n: &mut usize, sample: u64) {
    buf[*n % RING] = sample;
    *n = n.saturating_add(1);
}

pub(super) fn window(buf: &[u64; RING], n: usize) -> Vec<u64> {
    let count = n.min(RING);
    let mut v = buf[..count].to_vec();
    v.sort_unstable();
    v
}

pub(super) fn percentile(sorted: &[u64], num: usize, den: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * num + den - 1) / den;
    sorted[(rank - 1).min(sorted.len() - 1)]
}

pub(super) fn nominal_tick_us(tick_rate_hz: u32) -> u64 {
    if tick_rate_hz == 0 {
        0
    } else {
        1_000_000 / tick_rate_hz as u64
    }
}
