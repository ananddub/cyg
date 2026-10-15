mod basics;
mod ceilings;
mod retry;
mod squeeze;

use std::net::IpAddr;

fn ip(n: u8) -> IpAddr {
    IpAddr::from([127, 0, 0, n])
}

/// A distinct address per call, so a flood can be generated without a nested
/// loop that quietly decides how much work the test does.
pub(super) struct SourceWalk {
    n: u32,
}

impl SourceWalk {
    pub(super) fn next(&mut self) -> IpAddr {
        self.n += 1;
        IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0000u32 + self.n))
    }
}

/// Fill the table with exhausted sources, keeping the walk so the test can mint new ones.
pub(super) fn flood_to_ceiling() -> (crate::infra::rate_limit::IpRateLimiter, SourceWalk) {
    let mut l = crate::infra::rate_limit::IpRateLimiter::with_rates(0.0001, 1.0);
    let mut walk = SourceWalk { n: 0 };
    let mut n = 0u32;
    while l.tracked() < crate::infra::rate_limit::MAX_BUCKETS && n < 200_000 {
        n += 1;
        l.allow(walk.next());
    }
    (l, walk)
}
