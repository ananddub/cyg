pub const TICK_HISTOGRAM_BUCKETS: usize = engine_bus::TICK_HISTOGRAM_BUCKETS;

pub fn bucket_edges_us() -> [u64; TICK_HISTOGRAM_BUCKETS] {
    let mut edges = [0u64; TICK_HISTOGRAM_BUCKETS];
    let mut i = 0;
    while i < TICK_HISTOGRAM_BUCKETS {
        edges[i] = 1u64 << (i + 1);
        i += 1;
    }
    edges
}

pub fn histogram_add(hist: &mut [u32; TICK_HISTOGRAM_BUCKETS], micros: u64) {
    let bucket = (63 - micros.max(1).leading_zeros()) as usize;
    hist[bucket.min(TICK_HISTOGRAM_BUCKETS - 1)] += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_buckets_by_power_of_two() {
        let mut h = [0u32; TICK_HISTOGRAM_BUCKETS];
        histogram_add(&mut h, 1); // [0,2) -> bucket 0
        histogram_add(&mut h, 3); // [2,4) -> bucket 1
        histogram_add(&mut h, 5_000); // over the top -> last bucket
        assert_eq!((h[0], h[1], h[TICK_HISTOGRAM_BUCKETS - 1]), (1, 1, 1));
        assert_eq!(h.iter().sum::<u32>(), 3);
    }

    #[test]
    fn bucket_edges_match_the_buckets() {
        let e = bucket_edges_us();
        assert_eq!(e[0], 2);
        assert_eq!(e[1], 4);
        assert_eq!(e[TICK_HISTOGRAM_BUCKETS - 1], 4096);
        assert_eq!(TICK_HISTOGRAM_BUCKETS, 12);
    }
}
