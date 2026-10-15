use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportMix {
    pub websocket: u32,
    pub webrtc: u32,
    pub native_udp: u32,
    pub signal_peers: u32,
    pub signals_delivered: u64,
    pub signals_dropped: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BusHealth {
    pub inputs_queued: usize,
    pub commands_queued: usize,
    pub deltas_queued: usize,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub inputs_dropped: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatHealth {
    pub outstanding: usize,
    pub minted: u64,
    pub rejected: u64,
    pub refused: u64,
    pub bound: usize,
    pub ttl_secs: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitHealth {
    pub allowed: u64,
    pub denied: u64,
    pub squeezed: u64,
    pub swept: u64,
    pub tracked: usize,
    pub rps: u32,
    pub burst: u32,
}
