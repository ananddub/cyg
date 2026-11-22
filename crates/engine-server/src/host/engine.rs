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
    pub(crate) rtc_input_rates: HashMap<std::rc::Rc<str>, netcode::InputRateLimiter>,
    pub(crate) message_rate_limits: MessageRateLimits,
    pub(crate) last_input_at: HashMap<std::rc::Rc<str>, Instant>,
}

impl Rates {
    /// Borrow-first update: an active session's key already exists, so the hot
    /// path mutates in place with no allocation; only the first input interns.
    pub(crate) fn note_input(&mut self, session: &str) {
        let now = Instant::now();
        if let Some(t) = self.last_input_at.get_mut(session) {
            *t = now;
        } else {
            self.last_input_at.insert(std::rc::Rc::from(session), now);
        }
    }
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
    pub(crate) session_scratch: Vec<(std::rc::Rc<str>, i64)>,
    /// Reused room-name snapshot for the tick fan-out, collected ONCE per
    /// tick interval (not per catch-up pass) — Arc clones only, each entry
    /// carrying the room's assigned stagger phase.
    pub(crate) room_names: Vec<(std::rc::Rc<str>, u32)>,
    pub(crate) delta_buf: Vec<u8>,
    pub(crate) view_buf: Vec<u8>,
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
    /// The phase slot that fires on the next stagger wake.
    pub(crate) next_phase: u32,
    /// Start of the currently-elapsed stagger slot.
    pub(crate) slot_base: Instant,
    /// Catch-up budget computed at the last interval boundary; each phase
    /// runs its rooms this many ticks once per interval.
    pub(crate) interval_pending: u32,
    /// Ranges of the schema sections inside `delta_buf`, parsed ONCE per
    /// room-tick by `tick_delta`. Senders re-slice instead of re-parsing —
    /// the same buffer was being split 5× per room-tick before.
    pub(crate) schema_full: std::ops::Range<usize>,
    pub(crate) schema_patch: std::ops::Range<usize>,
    /// Prebuilt wrapped delta frames (your_id=0, acked_seq=0), built once per
    /// room-tick in the packed lane. Per-session sends patch the 8-byte
    /// your_id + 2-byte acked_seq in place instead of re-wrapping the whole
    /// body per session.
    pub(crate) wrap_full_packed: Vec<u8>,
    pub(crate) wrap_patch_packed: Vec<u8>,
    /// Raw packed sections (before the `0xA5 "delta"` envelope) — the shared
    /// body used by the WS shared-send path.
    pub(crate) packed_full_body: Vec<u8>,
    pub(crate) packed_patch_body: Vec<u8>,
    /// Volatile-only packed patch (fields marked `.unreliable()`), sent on
    /// the lossy stream after the reliable state frame.
    pub(crate) packed_volatile_body: Vec<u8>,
    pub(crate) volatile_ready: bool,
    /// True when the volatile patch must be masked per session (owner-only or
    /// an active view), so the shared body cannot be reused.
    pub(crate) volatile_per_view: bool,
    /// Per-session AOI view re-encoded for the packed lane.
    pub(crate) packed_view_buf: Vec<u8>,
    /// The descriptor's packed schemas, compiled once at boot. `None` for relay
    /// rooms or a descriptor with no collection.
    pub(crate) packed_lane: Option<netcode::PackedSet>,
    /// The descriptor has an owner-only field, so the packed body depends on
    /// who is viewing it; the shared per-tick frame cannot be reused.
    pub(crate) private_state: bool,
    /// True when the current room tick produced packed frames.
    pub(crate) packed_ready: bool,
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
    /// Wire bytes this lane has read and written since boot. Phase 0 of the netcode work: you
    /// cannot prove a bandwidth change without a before number.
    pub(crate) wire_bytes_in: u64,
    pub(crate) wire_bytes_out: u64,
    /// Datagrams this lane had to split to stay inside the MTU.
    pub(crate) wire_fragments: u64,
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
    /// Authoritative server process lifecycle (boot/ready/draining/shutdown)
    /// plus the registry of managed process threads. Shared with the control
    /// plane so one authority owns both the phase and the threads.
    pub(crate) manager: Arc<engine_manager::ServerManager>,
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
