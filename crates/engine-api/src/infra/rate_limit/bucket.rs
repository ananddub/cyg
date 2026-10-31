use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;

pub const MAX_BUCKETS: usize = 16384;

pub(super) const MAKEROOM_SCAN_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(super) struct Bucket {
    pub(super) tokens: f64,
    pub(super) last: std::time::Instant,
}

pub struct IpRateLimiter {
    pub(super) refill_per_sec: f64,
    pub(super) burst: f64,
    pub(super) buckets: HashMap<IpAddr, Bucket>,
    pub(super) order: VecDeque<IpAddr>,
    pub(super) allowed: u64,
    pub(super) denied: u64,
    pub(super) squeezed: u64,
    pub(super) swept: u64,
}

impl IpRateLimiter {
    pub fn new(refill_per_sec: u32, burst: u32) -> IpRateLimiter {
        IpRateLimiter::with_rates(f64::from(refill_per_sec), f64::from(burst))
    }

    pub fn with_rates(refill_per_sec: f64, burst: f64) -> IpRateLimiter {
        IpRateLimiter {
            refill_per_sec: refill_per_sec.max(f64::EPSILON),
            burst: burst.max(1.0),
            buckets: HashMap::new(),
            order: VecDeque::new(),
            allowed: 0,
            denied: 0,
            squeezed: 0,
            swept: 0,
        }
    }
}
