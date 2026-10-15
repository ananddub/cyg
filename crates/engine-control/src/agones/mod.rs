//! Agones sidecar lifecycle.
//!
//! An Agones pod ships an SDK sidecar on `127.0.0.1:AGONES_SDK_HTTP_PORT`
//! (9358) with a tiny REST surface (`/ready`, `/allocated`, `/health`,
//! `/state`, `/shutdown`). The client that speaks it is [`engine_agones`];
//! this module is only the control plane's seam — it decides *where* the loop
//! runs and *what it prints*.
//!
//! The health loop is a plain OS thread with a `std::net` client rather than
//! a Tokio task: the sidecar's latency is unbounded (a throttled kubelet is a
//! real thing) and a blocking thread cannot stall an Axum worker.

mod beat;
mod loud;
mod side;

pub use beat::spawn;
pub use side::{shutdown, stop_heartbeats};
