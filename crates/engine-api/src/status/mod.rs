//! Status: the HTTP telemetry routes (`/status`, `/api/rooms`) and the queue
//! pump that keeps them fresh.
//!
//! The sim plane (`engine_server::telemetry`) publishes a `StatusSnapshot`
//! about once a second onto `bus.status_tx`. Exactly one task here — [`pump`] —
//! drains that queue and keeps the newest sample in a mutex, so `/status` never
//! races a second reader and never touches the sim thread.
//!
//! `updated_ms` in a snapshot is monotonic milliseconds from the process-wide
//! epoch in `engine_bus::clock`, which is what makes `age_ms` meaningful
//! without involving wall-clock skew.

mod pump;
mod rooms;
mod routes;
mod view;

pub use pump::pump;
pub use routes::router;

#[cfg(test)]
mod tests;
