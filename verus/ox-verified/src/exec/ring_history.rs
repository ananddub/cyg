// Verified mirror of the ring-buffer core of
// `crates/engine-core/src/rewind/history.rs` (`EntityHistory`).
//
// Keep in sync with the original. The f64-time interpolation logic
// (`find_samples`/`find_bounding`) is NOT mirrored here — floating-point
// verification is out of scope for this pass; what is verified is the ring
// structure itself: head/count/capacity coherence, the strictly-increasing
// tick invariant (stale-record rejection), and the logical->physical index
// math that `sample_at` performs.
//
// NOTE: `#[verus_spec]` cannot annotate impl methods in this Verus build, so
// all verified logic lives in free functions (`rh_*`) and the inherent impl
// is a thin delegate.

#[cfg_attr(verus, verus_verify)]
pub struct RingHistory<T> {
    pub ticks: Vec<u64>,
    pub data: Vec<T>,
    pub head: usize,
    pub count: usize,
    pub capacity: usize,
    pub last_tick: Option<u64>,
}

impl<T: Copy> RingHistory<T> {
    /// Same construction contract as `EntityHistory::with_interpolation`:
    /// capacity is clamped to at least 2.
    pub fn with_capacity(capacity: usize) -> Self {
        rh_new(capacity)
    }

    /// Number of samples currently stored.
    pub fn len(&self) -> usize {
        rh_len(self)
    }

    pub fn is_empty(&self) -> bool {
        rh_is_empty(self)
    }

    /// Record one sample. Stale records (`tick <= last_tick`) are dropped
    /// silently — exactly like the original — which is what maintains the
    /// strictly-increasing tick invariant.
    pub fn record(&mut self, tick: u64, data: T) {
        rh_record(self, tick, data)
    }

    /// Logical index -> physical slot, identical arithmetic to the original's
    /// `sample_at`: slot 0 is the OLDEST sample, `count - 1` the newest.
    pub fn sample_tick(&self, logical: usize) -> u64 {
        rh_sample_tick(self, logical)
    }
}

/// Verified core of `RingHistory::with_capacity`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    ensures
        r.capacity >= 2,
        r.head == 0,
        r.count == 0,
        r.ticks.len() == 0,
        r.data.len() == 0,
        r.last_tick.is_none(),
        rh_ok(&r),
))]
pub fn rh_new<T>(capacity: usize) -> RingHistory<T> {
    RingHistory {
        ticks: Vec::new(),
        data: Vec::new(),
        head: 0,
        count: 0,
        capacity: if capacity < 2 { 2 } else { capacity },
        last_tick: Option::None,
    }
}

/// Verified core of `RingHistory::len`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    returns h.count
))]
pub fn rh_len<T>(h: &RingHistory<T>) -> usize {
    h.count
}

/// Verified core of `RingHistory::is_empty`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    returns h.count == 0
))]
pub fn rh_is_empty<T>(h: &RingHistory<T>) -> bool {
    h.count == 0
}

/// Verified core of `RingHistory::record`. Stale records (`tick <=
/// last_tick`) are dropped silently — exactly like the original — which is
/// what maintains the strictly-increasing tick invariant.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        rh_ok(h),
        h.capacity <= 1073741824usize,
    ensures
        final(h).head < final(h).capacity &&
        final(h).count <= final(h).capacity &&
        final(h).ticks.len() == final(h).count &&
        final(h).data.len() == final(h).count &&
        final(h).capacity >= 2 &&
        (final(h).head == final(h).count || final(h).count == final(h).capacity),
        (old(h).last_tick.is_some() && tick <= old(h).last_tick.unwrap_or(0u64)) ==>
            *final(h) == *old(h),
        (old(h).last_tick.is_none() || tick > old(h).last_tick.unwrap_or(0u64)) ==> {
            final(h).last_tick == Option::Some(tick) &&
            (old(h).count == old(h).capacity ==> final(h).count == old(h).count) &&
            (old(h).count < old(h).capacity ==> final(h).count == old(h).count + 1)
        },
))]
pub fn rh_record<T: Copy>(h: &mut RingHistory<T>, tick: u64, data: T) {
    if let Some(last) = h.last_tick {
        if tick <= last {
            return;
        }
    }
    h.last_tick = Option::Some(tick);
    if h.count < h.capacity {
        h.ticks.push(tick);
        h.data.push(data);
        h.head = if h.head + 1 == h.capacity { 0 } else { h.head + 1 };
        h.count += 1;
        assert!(h.head < h.capacity);
        assert!(h.head == h.count || h.count == h.capacity);
        assert!(h.ticks.len() == h.count);
    } else {
        h.ticks[h.head] = tick;
        h.data[h.head] = data;
        h.head = if h.head + 1 == h.capacity { 0 } else { h.head + 1 };
        assert!(h.head < h.capacity);
        assert!(h.count == h.capacity);
        assert!(h.ticks.len() == h.count);
    }
}

/// Verified core of `RingHistory::sample_tick` — logical index -> physical
/// slot, identical arithmetic to the original's `sample_at`: slot 0 is the
/// OLDEST sample, `count - 1` the newest.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        rh_ok(h),
        logical < h.count,
        h.capacity <= 1073741824usize,
    ensures
        (h.head == h.count ==> r == h.ticks@[logical as int]) &&
        (h.head != h.count ==>
            r == h.ticks@[((h.head as int + logical as int) % (h.capacity as int))]),
))]
pub fn rh_sample_tick<T>(h: &RingHistory<T>, logical: usize) -> u64 {
    // Same value as the original's single modulo expression, split on the
    // ring's two states (pre-wrap: slots stored contiguously; wrapped: head
    // is the oldest slot) so Verus can prove the bounds on each branch.
    let phys = if h.head == h.count {
        logical
    } else {
        (h.head + logical) % h.capacity
    };
    h.ticks[phys]
}
