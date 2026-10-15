use serde::Serialize;

use crate::models::health::{BusHealth, RateLimitHealth, SeatHealth, TransportMix};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub server: ServerInfo,
    pub uptime_sec: u64,
    pub ccu: u32,
    pub rooms: u32,
    pub capacity: Capacity,
    pub tick: TickHealth,
    pub transport: TransportMix,
    pub bus: BusHealth,
    pub seats: SeatHealth,
    pub rate_limit: RateLimitHealth,
    pub agones: serde_json::Value,
    pub has_sim_sample: bool,
    pub sample_age_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub port: u16,
    pub control_port: u16,
    pub web_port: u16,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capacity {
    pub players: u32,
    pub max: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickHealth {
    pub hz: f64,
    pub configured_hz: u32,
    pub nominal_us: u64,
    pub p50_us: u64,
    pub p99_us: u64,
    pub overruns: u64,
    pub samples: u32,
    pub seq: u64,
    pub histogram: [u32; crate::utils::histogram::TICK_HISTOGRAM_BUCKETS],
}
