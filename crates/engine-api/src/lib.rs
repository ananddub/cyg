//! # engine-api — the browser-facing surface
//!
//! Everything an HTTP or WebSocket client touches, and nothing else: the route
//! composition ([`api`]), the shared handler state ([`ApiState`]), seat tokens
//! ([`seats`]), the matchmake rate limiter ([`rate_limit`]), WebRTC signaling
//! ([`signaling`]), the telemetry routes ([`status`]), the operator dashboard
//! ([`dashboard`]) and the project's static website ([`static_routes`]).
//!
//! It owns no runtime, no listener and no simulation. `engine-control` builds
//! an [`ApiState`] for the port it bound, hands it to [`serve`], and spawns
//! [`pump`](status::pump) and [`gc_seats`] on that runtime. Nothing here blocks
//! on the sim plane: every bus touch is a `try_send`, and every queue is
//! bounded and drops instead of stalling a client's socket task.

pub mod dashboard;
pub mod matchmake;
pub mod rate_limit;
pub mod seats;
pub mod signaling;
pub mod static_routes;
pub mod status;

mod api;
mod error;
mod gc;
mod serve;
mod state;

pub use api::router;
pub use error::ApiError;
pub use gc::gc_seats;
pub use serve::serve;
pub use state::{ApiState, OnceReceiver};
