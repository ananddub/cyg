//! Bounded crossbeam channels shared by the Tokio control plane and the
//! pinned sim thread. Producers never block; full queues drop data.

mod bus;
pub mod clock;
mod messages;
mod telemetry;

pub use bus::Bus;
pub use messages::{
    FrameKind, InputMsg, MapReload, MatchFinished, OutFrame, PeerId, ReloadKind, RoomSnapshot,
    SettingsReload, SimCommand, SimEvent, SnapshotFetch, Via, WasmReload,
};
pub use telemetry::{
    RoomPhase, RoomTelemetry, SessionTelemetry, StatusSnapshot, TICK_HISTOGRAM_BUCKETS,
};
