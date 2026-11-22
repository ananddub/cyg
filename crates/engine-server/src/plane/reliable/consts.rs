use engine_webrtc_protocol::EVENT_WIRE_LEN;

pub const MAX_INFLIGHT_EVENTS: usize = 32;
pub const MIN_RTO: u64 = 40;
pub const MAX_RTO: u64 = 2_000;
pub const INITIAL_RTO: u64 = 150;
pub const MAX_BACKOFF_SHIFT: u32 = 3;
/// Initial congestion window in events. Matches the hard inflight cap so the
/// first burst behaves exactly as before; loss then shrinks it.
pub const INITIAL_CWND: f64 = MAX_INFLIGHT_EVENTS as f64;
/// Never shrink below one event: the lane must still make progress.
pub const MIN_CWND: f64 = 1.0;
pub const EVENT_LIFETIME: u64 = 10_000;
pub const MAX_EVENT_BYTES: usize = EVENT_WIRE_LEN;
pub const MAX_RELIABLE_BYTES: usize = 2048;
