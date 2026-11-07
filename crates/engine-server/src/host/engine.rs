use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use engine_bus::Bus;
use engine_core::MessageRateLimits;
use engine_wasm::{EpochTicker, WasmRuntime};

use crate::schema_view::SchemaAoi;

use crate::host::*;
use engine_config::{GameConfig, GameplayConfig};

pub(crate) struct Scratch {
    pub(crate) events: Vec<TransportEvent>,
}

pub(crate) struct Rates {
    pub(crate) rtc_input_rates: HashMap<String, netcode::InputRateLimiter>,
    pub(crate) message_rate_limits: MessageRateLimits,
    pub(crate) last_input_at: HashMap<String, Instant>,
}

pub(crate) struct Clock {
    pub(crate) tick_interval: Duration,
}

pub(crate) struct Stats {
    pub(crate) recorder: Recorder,
    pub(crate) publish_start: Instant,
}

pub(crate) struct TickRunner {
    /// Reused per-room session list — avoids re-allocating every tick.
    pub(crate) session_scratch: Vec<(std::sync::Arc<str>, i64)>,
    pub(crate) delta_buf: Vec<u8>,
    pub(crate) view_buf: Vec<u8>,
    pub(crate) frame_buf: Vec<u8>,
    pub(crate) delta_body: Vec<u8>,
    pub(crate) filtered_buf: Vec<u8>,
    pub(crate) udp_batch: Vec<(SocketAddr, Vec<u8>)>,
    pub(crate) schema_aoi: Option<SchemaAoi>,
    pub(crate) dead_rooms: Vec<(String, CloseReason)>,
    pub(crate) last_tick: Instant,
    pub(crate) tick_counter: u64,
    pub(crate) tick_stat_start: Instant,
    pub(crate) window_ticks: u64,
    pub(crate) stage_sim_us_sum: u64,
    pub(crate) stage_guest_us_sum: u64,
    pub(crate) match_peaks: HashMap<String, u32>,
}

pub(crate) struct TickCtx<'e> {
    pub(crate) server: &'e mut GameServer,
    pub(crate) transport: &'e mut LoopTransport,
    pub(crate) udp: &'e mut UdpLane,
    pub(crate) bus: &'e Arc<Bus>,
    pub(crate) game_cfg: &'e Arc<GameplayConfig>,
    pub(crate) cfg: &'e GameConfig,
    pub(crate) game_id: &'e str,
    pub(crate) node_id: &'e str,
    pub(crate) recorder: &'e mut Recorder,
    pub(crate) tick_interval: Duration,
}

pub(crate) struct HotReload<'a> {
    pub(crate) runtime: &'a mut WasmRuntime,
    pub(crate) project_dir: &'a Path,
    pub(crate) wasm_path: &'a Path,
    pub(crate) source_path: PathBuf,
    pub(crate) console_active: bool,
    pub(crate) wasm_available: bool,
    pub(crate) last_wasm_check: Instant,
    pub(crate) loaded_wasm_time: Option<SystemTime>,
    pub(crate) loaded_src_time: Option<SystemTime>,
}

pub(crate) struct UdpLane {
    pub(crate) udp_sock: Option<Arc<std::net::UdpSocket>>,
    pub(crate) udp_peers: HashMap<SocketAddr, (i64, String, Instant)>,
    pub(crate) udp_reliable: HashMap<SocketAddr, crate::plane::reliable::ReliableLane>,
    pub(crate) udp_rxd_seq: HashMap<SocketAddr, std::collections::HashSet<u16>>,
    pub(crate) udp_input_rates: HashMap<SocketAddr, netcode::InputRateLimiter>,
    pub(crate) udp_join_rates: HashMap<SocketAddr, netcode::InputRateLimiter>,
    pub(crate) rudp_scratch: Vec<u8>,
    pub(crate) schema_descriptor: Vec<u8>,
    pub(crate) timed_out_udp: Vec<(SocketAddr, i64, String)>,
}

pub(crate) struct UdpCtx<'e> {
    pub(crate) server: &'e mut GameServer,
    pub(crate) transport: &'e mut LoopTransport,
    pub(crate) bus: &'e Arc<Bus>,
    pub(crate) game_cfg: &'e Arc<GameplayConfig>,
    pub(crate) cfg: &'e GameConfig,
    pub(crate) recorder: &'e mut Recorder,
    pub(crate) tick_interval: Duration,
}

pub(crate) struct Engine<'a> {
    pub(crate) cfg: &'a GameConfig,
    pub(crate) bus: Arc<Bus>,
    pub(crate) game_cfg: Arc<GameplayConfig>,
    pub(crate) _epoch_ticker: Option<EpochTicker>,
    pub(crate) hot: HotReload<'a>,
    pub(crate) signals: &'a Option<SignalHub>,
    pub(crate) game_id: String,
    pub(crate) node_id: String,
    pub(crate) transport: LoopTransport,
    pub(crate) server: GameServer,
    pub(crate) arena_mesh: Option<engine_physics::oxcl::CookedMesh>,
    pub(crate) udp: UdpLane,
    pub(crate) tick: TickRunner,
    pub(crate) signal_rx: Option<tokio::sync::mpsc::Receiver<SignalIn>>,
    pub(crate) drain: Drain,
    pub(crate) scratch: Scratch,
    pub(crate) rates: Rates,
    pub(crate) clock: Clock,
    pub(crate) stats: Stats,
    /// The schema's `JoinOptions` fields, in wire order — join options are
    /// re-encoded into this frame so the guest's `onJoin(id, options)` is
    /// typed. Empty when the schema declares no `JoinOptions` def (raw JSON
    /// passthrough instead).
    pub(crate) join_options_fields: Vec<engine_schema::SchemaField>,
}
