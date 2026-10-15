//! # engine-control — the control plane's runtime
//!
//! Owns the Tokio runtime, the bound listener, and the plane's background
//! tasks: serving the API, the status pump, the seat sweep, and the Agones
//! sidecar lifecycle. Everything browser-facing — routes, handlers, seat
//! tokens, signaling queues, the dashboard — lives in [`engine_api`].
//!
//! ## Port split
//!
//! The game WebSocket stays on `cfg.port` inside `engine-net`'s transport; this
//! plane binds `cfg.control.api_port`. Two listeners, one port each, so a slow
//! HTTP handler can stall neither the 128 Hz tick nor the WS accept loop.

pub mod agones;

mod plane;
mod run;

pub use plane::ControlPlane;
pub use run::spawn;
