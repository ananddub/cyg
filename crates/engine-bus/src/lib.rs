//! Bounded crossbeam channels shared by the Tokio control plane and the
//! pinned sim thread. Producers never block; full queues drop data.

mod bus;
pub mod clock;
mod messages;
mod telemetry;

pub use bus::Bus;
pub use messages::{
    FrameKind, InputMsg, MapReload, OutFrame, PeerId, ReloadKind, SettingsReload, SimCommand, Via,
    WasmReload,
};
pub use telemetry::{SessionTelemetry, StatusSnapshot, TICK_HISTOGRAM_BUCKETS};
