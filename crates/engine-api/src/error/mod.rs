//! Handler failures, in the two shapes this crate speaks.
//!
//! [`V2Error`] is the `/v2` API's: an HTTP status carrying the failure kind plus a
//! `{"error":{"code":…,"message":…,"details":[…]}}` body. [`ApiError`] is the
//! browser-facing `{"error": …}` body a page can read.

mod api;
mod v2;

pub use api::ApiError;
pub use v2::*;

#[cfg(test)]
mod tests;
