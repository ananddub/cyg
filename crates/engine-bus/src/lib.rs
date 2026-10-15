//! # engine-bus — typed, bounded crossbeam channels between the two planes
//!
//! * the **control plane** (Tokio): Axum HTTP, WS signaling, WebRTC sessions;
//! * the **sim plane** (a pinned MIO thread): raw UDP and the fixed-rate
//!   simulation — no Tokio, no locks on room state.
//!
//! Producers `try_send` and never block. When full: `inputs` drop the oldest,
//! `deltas` drop the frame, `commands` refuse, `status` drops the snapshot.
//!
//! All shared message types live here; other crates import them instead of
//! defining parallel types.

mod bus;
pub mod clock;
mod messages;
mod telemetry;

pub use bus::Bus;
pub use messages::{FrameKind, InputMsg, OutFrame, PeerId, SimCommand, Via};
pub use telemetry::{SessionTelemetry, StatusSnapshot, TICK_HISTOGRAM_BUCKETS};
