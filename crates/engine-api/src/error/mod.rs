//! Handler failures, in the two shapes this crate speaks.
//!
//! [`V2Error`] is the `/v2` API's: an HTTP status carrying the failure kind plus a
//! `{"error":{"code":…,"message":…,"details":[…]}}` body. [`ApiError`] is the
//! browser-facing shape, the same envelope with the `code` derived from the status.

mod api;
mod v2;

pub use api::ApiError;
pub use v2::*;

#[cfg(test)]
mod tests;
