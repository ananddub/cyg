use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;

/// Distinct buckets kept before the ceiling bites. At the ceiling, refilled
/// buckets are dropped and a brand-new source is refused rather than evicting
/// somebody's exhausted bucket (which would hand them a free reset).
pub const MAX_BUCKETS: usize = 16384;

/// Buckets examined per `make_room` call. The ceiling has to bite in constant
/// time: sweeping all of `MAX_BUCKETS` on every admission made the limiter
/// itself the most expensive object under a spoofed-source flood. A bounded
/// rotation still reaches every tracked bucket within
/// `MAX_BUCKETS / MAKEROOM_SCAN_BUDGET` admissions.
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
    /// First-seen order, used as a rotation ring by `make_room`. One entry per
    /// tracked IP, so it stays bounded by `MAX_BUCKETS`.
    pub(super) order: VecDeque<IpAddr>,
    pub(super) allowed: u64,
    pub(super) denied: u64,
    /// Requests refused because the tracked-source ceiling was full. A capacity
    /// event, not an abuse event: counted apart from `denied`.
    pub(super) squeezed: u64,
    /// Buckets examined while making room at the ceiling; the counter exists so
    /// a test can prove the per-call bound.
    pub(super) swept: u64,
}

impl IpRateLimiter {
    pub fn new(refill_per_sec: u32, burst: u32) -> IpRateLimiter {
        IpRateLimiter::with_rates(f64::from(refill_per_sec), f64::from(burst))
    }

    /// The same bucket with a fractional rate, so a deployment can police
    /// below 1 rps and `retry_after` is testable at honest rates.
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
