//! What the publishing loop contributes beyond its own timing windows.

use engine_bus::SessionTelemetry;

/// The session facts only the loop that owns the sockets can know. A struct
/// rather than positional args because the host loop, the pinned sim plane and
/// a test each fill a different subset, and five adjacent `u32`s cannot show
/// which.
#[derive(Clone, Debug, Default)]
pub struct PublishReport {
    /// Measured tick rate over the publish window.
    pub tick_hz_est: f64,
    /// The configured rate (`GameConfig::tick_rate`). 0 leaves
    /// `nominal_tick_us` at 0 rather than inventing a period.
    pub tick_rate_hz: u32,
    /// Room members, spectators excluded.
    pub players: u32,
    /// Sessions counted on the transport their packets actually arrive on, so
    /// the three sum to the connected clients rather than double-counting a
    /// browser socket that upgraded to a data channel.
    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,
    /// One row per live session; the recorder truncates it to `session_cap`.
    pub sessions: Vec<SessionTelemetry>,
    /// Ceiling on `sessions`, normally the room's `max_clients`. 0 means the
    /// contract ceiling `MAX_CLIENT_ROWS` alone.
    pub session_cap: usize,
}
