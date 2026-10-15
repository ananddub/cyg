//! Per-source token bucket.
//!
//! Matchmake is the one endpoint an unauthenticated client can hammer, and
//! each call costs an HMAC plus a map entry, so the abuse cost lands on the
//! server. One bucket per client IP, refilled lazily on access — no timer, no
//! background task, one `HashMap` behind a mutex.
//!
//! Only the request's *socket* address is charged. A `X-Forwarded-For` header
//! is client-supplied and would let a caller pick which bucket it bypasses; if
//! this plane is ever put behind a proxy, the proxy becomes the rate limiter.
//!
//! Memory is bounded like the seat map: at most [`MAX_BUCKETS`] addresses are
//! tracked, and making room at the ceiling is a *bounded* rotation, never a
//! full sweep — a ceiling whose admission path is O(tracked sources) is a
//! denial-of-service against the limiter itself.

mod admit;
mod bucket;
mod read;

pub use bucket::{IpRateLimiter, MAX_BUCKETS};

#[cfg(test)]
mod tests;
