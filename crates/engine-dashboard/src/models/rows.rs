use serde::Serialize;

pub const MAX_CLIENT_ROWS: usize = 256;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRow {
    pub name: String,
    pub clients: u32,
    pub max_clients: u32,
    pub tick_hz: f64,
    pub tick_rate: u32,
    pub port: u16,
    pub control_port: u16,
    pub empty_for_sec: Option<u64>,
    pub status: String,
    pub has_sim_sample: bool,
    pub sample_age_ms: Option<u64>,
    pub signaling_url: String,
    pub server_id: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRow {
    pub name: String,
    pub kind: String,
    pub threads: u32,
    pub ccu: u32,
    pub tick_p50_us: Option<u64>,
    pub tick_p99_us: Option<u64>,
    pub nominal_tick_us: Option<u64>,
    pub overruns: Option<u64>,
    pub deltas_sent: Option<u64>,
    pub deltas_dropped: Option<u64>,
    pub queue_depth: Option<usize>,
    pub live: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientRow {
    pub session: String,
    pub transport: String,
    pub room: String,
    pub player_id: Option<i64>,
    pub acked_seq: Option<u32>,
    pub idle_ms: Option<u64>,
}
