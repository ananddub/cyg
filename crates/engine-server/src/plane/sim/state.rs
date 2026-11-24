use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_bus::{Bus, OutFrame, PeerId};
use engine_config::AoiConfig;
use engine_wasm::EpochTicker;
use mio::{Events, Poll};

use crate::plane::PlaneConfig;
use crate::plane::udp::UdpPlane;
use crate::rooms::GameRoom;
use crate::telemetry::Recorder;

pub(super) struct Plane {
    pub(super) bus: Arc<Bus>,
    pub(super) cfg: PlaneConfig,
    pub(super) udp: UdpPlane,
    pub(super) poll: Poll,
    pub(super) room: GameRoom,
    pub(super) _epoch_ticker: Option<EpochTicker>,
    pub(super) recorder: Recorder,
    pub(super) events: Events,
    pub(super) period: Duration,
    pub(super) dt: f64,
    pub(super) aoi_cfg: AoiConfig,
    pub(super) plane_game_cfg: engine_config::GameConfig,
    pub(super) node_id: String,
    pub(super) last_tick: Instant,
    pub(super) sweep_start: Instant,
    pub(super) status_start: Instant,
    pub(super) window_ticks: u64,
    pub(super) stage_sim_us_sum: u64,
    pub(super) stage_guest_us_sum: u64,
    pub(super) measured_hz: f64,
    pub(super) native_send_failed: u64,
    pub(super) browser_players: HashSet<i64>,
    pub(super) session_of: HashMap<i64, PeerId>,
    pub(super) last_input_at: HashMap<PeerId, Instant>,
    pub(super) last_pos: HashMap<i64, [f64; 3]>,
    pub(super) delta_buf: Vec<u8>,
    pub(super) filtered_buf: Vec<u8>,
    /// Reused AOI visibility scratch — cleared/resized per viewer by
    /// filter_for_viewer_indexed instead of allocating a Vec<bool> per call.
    pub(super) visible_buf: Vec<bool>,
    pub(super) scratch: Vec<u8>,
    pub(super) frames: Vec<OutFrame>,
    pub(super) tick_t0: Instant,
    pub(super) trap: bool,
}
