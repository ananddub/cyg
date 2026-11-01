
use engine_bus::{RoomTelemetry, SessionTelemetry};

#[derive(Clone, Debug, Default)]
pub struct PublishReport {

    pub tick_hz_est: f64,

    pub stage_sim_us: u64,

    pub stage_guest_us: u64,

    pub tick_rate_hz: u32,

    pub players: u32,

    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,

    pub sessions: Vec<SessionTelemetry>,

    pub rooms: Vec<RoomTelemetry>,

    pub session_cap: usize,
}
