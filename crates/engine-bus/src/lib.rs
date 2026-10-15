//! # The bus: typed bounded crossbeam channels between the two planes
//!
//! Topology (locked plan, Agent 3):
//!
//! * **Tokio async runtime** — Axum control plane, WS signaling, WebRTC
//!   (str0m) sessions. Everything that needs crypto or blocking I/O.
//! * **One MIO OS thread, pinned** — raw UDP :7778 + the 128 Hz simulation.
//!   Zero Tokio, zero crypto, zero locks on room state.
//!
//! The only shared surface is this module: producers `try_send` into bounded
//! channels and *never* block. Unreliable deltas are dropped when a plane is
//! behind (the next tick supersedes them anyway); inputs are dropped oldest
//! first when a queue is full, which is exactly what an unreliable input
//! stream tolerates.
//!
//! Rule for every agent: **shared message types live here and only here.**
//! Import them; do not invent parallel types.
//!
//! Moved verbatim from the root binary's `src/bus.rs`; downstream crates
//! reference these names as `engine_bus::<name>`.

use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use serde::Serialize;

/// Opaque per-connection identifier used on the bus. For the raw UDP plane it
/// is the session string minted at seat-token bind time; the RTC plane will
/// use its DataChannel session id. Room membership maps it to a player id.
pub type PeerId = String;

/// One client input traveling planes → sim.
///
/// `payload` is the fixed 20-byte guest input blob (`wasm.on_input(id, ptr, 20)`
/// in `src/server.rs`); planes never interpret it.
#[derive(Clone, Debug)]
pub struct InputMsg {
    /// Player id already assigned by the sim (bound at join time).
    pub player_id: i64,
    /// Client-side input sequence, echoed back as `acked_seq` in the delta tail.
    pub seq: u16,
    pub payload: [u8; 20],
    /// `mono_ns()` captured at the moment the datagram was read from the kernel.
    /// The sim subtracts it when it drains, giving kernel→sim latency.
    pub arrival_ns: u64,
    pub via: Via,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    NativeUdp,
    Browser,
}

/// Control/telemetry channel kinds for an outbound frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// game-unreliable / raw UDP deltas: fragment-stamped, newest-wins.
    Unreliable,
    /// game-reliable / control replies (BIND, DENY, events): never dropped.
    Reliable,
}

/// One fully framed, transport-ready outbound datagram/message. The sim builds
/// these (channel layer already applied: `OP_DELTA` header, seq, tick,
/// fragmentation) and hands them to the owning plane. `bytes` is the complete
/// frame as it must appear on the wire.
#[derive(Clone, Debug)]
pub struct OutFrame {
    pub peer_id: PeerId,
    pub kind: FrameKind,
    pub bytes: Vec<u8>,
}

/// Everything the sim accepts from the other plane(s).
#[derive(Debug)]
pub enum SimCommand {
    /// A plane verified a seat token and wants this peer bound into `room`.
    /// The sim assigns the player id (`on_join`) and sends it back through
    /// `reply` (bounded(1), same-thread handoff — never blocks).
    BindPeer {
        peer_id: PeerId,
        room: String,
        reply: Sender<i64>,
    },
    /// Peer disconnected or timed out; sim releases the player slot.
    DropPeer { peer_id: PeerId },
    /// Reliable event to deliver to one peer (or all when `target` is None).
    Event {
        room: String,
        target: Option<PeerId>,
        bytes: Vec<u8>,
    },
}

/// Buckets in [`StatusSnapshot::tick_histogram`]: bucket `i` counts ticks that
/// ran `[2^i, 2^(i+1))` microseconds, and the last bucket also soaks up anything
/// longer. 12 buckets reach 4.096 ms, which is one tick's budget at 128 Hz.
pub const TICK_HISTOGRAM_BUCKETS: usize = 12;

/// One connected client, as the host loop saw it at publish time.
///
/// The host owns sessions; the control plane may not reach into a running room.
/// So the host describes them here, once a second, and `/api/dashboard/clients`
/// renders them. `transport` is `websocket` | `webrtc` | `native-udp`.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTelemetry {
    pub session: String,
    pub transport: String,
    pub player_id: Option<i64>,
    /// Highest input seq applied from this session — the same number the client
    /// sees echoed in its delta tail.
    pub acked_seq: Option<u32>,
    /// Milliseconds since this session's last input; `None` if it never sent one.
    pub idle_ms: Option<u64>,
}

/// Sim → status sink: latest-wins snapshot for `/status` and logs.
/// Plain data (no atomics needed on the reader side); the sim publishes at a
/// low rate (once per second), never on the hot path.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StatusSnapshot {
    pub tick_hz_est: f64,
    pub tick_budget_p50_us: u64,
    pub tick_budget_p99_us: u64,
    pub tick_overruns: u64,
    pub kernel_to_sim_p50_us: u64,
    pub kernel_to_sim_p99_us: u64,
    pub players: u32,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub inputs_dropped: u64,
    pub seq: u64,
    pub updated_ms: u64,
    /// Sessions per transport at publish time. The three sum to [`players`]
    /// only when every client is counted once; a session that upgraded WS →
    /// WebRTC is counted where its packets actually arrive.
    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,
    /// Ticks whose duration landed in this publish window. Zero means the
    /// window was empty, which is how a reader tells "no samples yet" from
    /// "every tick was instant".
    pub tick_samples: u32,
    /// Base-2 histogram of tick durations over the window, oldest-window-wins:
    /// the sim's recorder resets it every publish, so it always describes the
    /// second the reader is looking at.
    pub tick_histogram: [u32; TICK_HISTOGRAM_BUCKETS],
    /// Nominal period of one tick at the configured rate, microseconds.
    /// `tick_budget_p99_us` against this is the "are we keeping up" number.
    pub nominal_tick_us: u64,
    /// Seconds the room has been empty. An operator's "why is this server up"
    /// field, and the only signal a idle process is not merely unvisited.
    pub empty_for_sec: u64,
    /// Per-session rows, capped by the producer at a room's `max_clients`.
    /// Empty means the host published nothing, not that nobody is connected —
    /// read it alongside [`StatusSnapshot::players`].
    pub sessions: Vec<SessionTelemetry>,
}

/// The whole inter-plane wiring, cheap to `Arc::clone`.
pub struct Bus {
    /// planes → sim: client inputs (bounded; drop-oldest when full).
    pub inputs_tx: Sender<InputMsg>,
    pub inputs_rx: Receiver<InputMsg>,
    /// planes/control → sim: bind/drop/event commands.
    pub commands_tx: Sender<SimCommand>,
    pub commands_rx: Receiver<SimCommand>,
    /// sim → RTC/browser plane: pre-framed delta/event messages (bounded;
    /// drop-whole-delta when full — the next tick supersedes it).
    pub deltas_tx: Sender<OutFrame>,
    pub deltas_rx: Receiver<OutFrame>,
    /// sim → status sink (bounded(8), latest published wins).
    pub status_tx: Sender<StatusSnapshot>,
    pub status_rx: Receiver<StatusSnapshot>,
}

impl Bus {
    pub fn new(input_cap: usize, delta_cap: usize) -> Arc<Bus> {
        let (inputs_tx, inputs_rx) = bounded(input_cap);
        let (commands_tx, commands_rx) = bounded(256);
        let (deltas_tx, deltas_rx) = bounded(delta_cap);
        let (status_tx, status_rx) = bounded(8);
        Arc::new(Bus {
            inputs_tx,
            inputs_rx,
            commands_tx,
            commands_rx,
            deltas_tx,
            deltas_rx,
            status_tx,
            status_rx,
        })
    }

    /// Non-blocking input enqueue. On full, drops the oldest and retries once —
    /// an unreliable stream tolerates losing the stale input, never the fresh.
    pub fn push_input(&self, msg: InputMsg) {
        let msg = match self.inputs_tx.try_send(msg) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(m)) => m,
            Err(_) => return,
        };
        let _ = self.inputs_rx.try_recv();
        let _ = self.inputs_tx.try_send(msg);
    }
}
