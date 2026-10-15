//! # engine-api — the browser-facing surface
//!
//! Everything an HTTP or WebSocket client touches: route composition ([`router`],
//! [`app`]), the handler state ([`ApiState`]), seat tokens ([`infra::seats`]), the
//! matchmake rate limiter ([`infra::rate_limit`]), WebRTC signaling
//! ([`infra::signaling`]), telemetry ([`crate::services::status`]), the dashboard
//! ([`crate::controllers::dashboard`]) and the static website
//! ([`crate::services::static_routes`]).
//!
//! No runtime of its own: `engine-control` builds an [`ApiState`] for the port it
//! bound, hands it to [`serve`], and spawns the status pump and [`gc_seats`] on
//! that runtime. Every bus touch is a `try_send`; every queue is bounded and drops
//! instead of stalling a client's socket task.

pub mod app;
pub mod controllers;
pub mod dto;
pub mod error;
pub mod infra;
pub mod repositories;
pub mod services;
pub mod utils;

#[cfg(test)]
mod tests;

pub use app::api::router;
pub use app::serve::serve;
pub use app::state::{ApiState, OnceReceiver, auto_router, current_state, install_state};
pub use error::ApiError;
pub use infra::gc::gc_seats;
pub use infra::pump::pump;
