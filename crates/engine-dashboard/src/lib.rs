//! # Engine Dashboard
//!
//! The wire contract for the control dashboard, plus the reader-side history
//! ring the live graphs draw from.
//!
//! This crate deliberately holds *no* Axum. It is the single source of truth
//! for what `/api/dashboard/*` and `/ws/dashboard` put on the wire, so the
//! routes in `engine-control`, the Rust tests here, and `dashboard/src/lib/
//! types.ts` on the frontend all describe the same object. A field that is not
//! in this file cannot be invented by a handler, and a field an agent adds here
//! must be filled in everywhere or removed.
//!
//! ## What is honest telemetry and what is not
//!
//! Every value here comes from a counter someone already pays to maintain:
//! `StatusSnapshot` from the sim loop, the bus' own queue depths, the seat
//! registry, the rate limiter, the signaling hub and the Agones sidecar. RTT
//! and jitter are *not* among them — the server has never measured a round
//! trip, the client does (`sdk/web/openoxide.js`'s `TimeSync`) — so this
//! contract carries no `rttMs`. A dashboard that shows a made-up number is
//! worse than one that shows nothing.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use serde::Serialize;

pub use engine_bus::StatusSnapshot;

/// Samples kept for the graphs: 300 s at one sample per second, which is five
/// minutes of 128 Hz history in 300 × a few hundred bytes.
pub const HISTORY_LEN: usize = 300;

/// Buckets in the tick histogram: bucket `i` covers `[2^i, 2^(i+1))` µs and the
/// last one also absorbs anything longer. Re-exported so a handler can label the
/// axis without importing `engine_bus`.
pub const TICK_HISTOGRAM_BUCKETS: usize = engine_bus::TICK_HISTOGRAM_BUCKETS;

/// Upper bound on client rows in one payload. A room is capped by
/// `max_clients`, but a runaway registry must not be able to size a response.
pub const MAX_CLIENT_ROWS: usize = 256;

/// A dashboard's whole first screen: one object, no joining on the client.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub server: ServerInfo,
    /// Wall-clock seconds this control plane has been up.
    pub uptime_sec: u64,
    /// Concurrent connected clients across every transport.
    pub ccu: u32,
    /// Always 1 in this topology — one room per process — but a dashboard
    /// grid needs the count to exist rather than to be inferred.
    pub rooms: u32,
    pub capacity: Capacity,
    pub tick: TickHealth,
    pub transport: TransportMix,
    pub bus: BusHealth,
    pub seats: SeatHealth,
    pub rate_limit: RateLimitHealth,
    /// The Agones sidecar's own snapshot, passed through untouched. Shape is
    /// that crate's business; `enabled: false` is the answer outside Kubernetes.
    pub agones: serde_json::Value,
    /// `false` when the sim has never published, in which case every `tick`
    /// field below reads as zero and the client must show "no sample".
    pub has_sim_sample: bool,
    /// How stale `tick` is, measured against the sim's monotonic epoch.
    pub sample_age_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    /// The game WebSocket/UDP port (`GameConfig::port`).
    pub port: u16,
    /// The port this plane actually bound.
    pub control_port: u16,
    pub web_port: u16,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capacity {
    pub players: u32,
    pub max: u32,
}

/// Tick health, straight off [`StatusSnapshot`]. `_us` fields are microseconds.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickHealth {
    /// Measured tick rate over the publish window.
    pub hz: f64,
    /// What the project asked for (`GameConfig::tick_rate`).
    pub configured_hz: u32,
    /// Nominal period of one tick at the configured rate.
    pub nominal_us: u64,
    pub p50_us: u64,
    pub p99_us: u64,
    /// Ticks that overran `nominal_us` since the process started.
    pub overruns: u64,
    /// Ticks measured in the current window; 0 means "no samples yet".
    pub samples: u32,
    /// Publish counter for this snapshot. Two polls with the same `seq` mean the
    /// sim stopped publishing, which is not the same as a flat line.
    pub seq: u64,
    /// Base-2 duration histogram for the window, for the bar chart.
    pub histogram: [u32; TICK_HISTOGRAM_BUCKETS],
}

/// Where the connections actually terminate.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportMix {
    pub websocket: u32,
    pub webrtc: u32,
    pub native_udp: u32,
    /// Live `/ws/signal` sockets.
    pub signal_peers: u32,
    pub signals_delivered: u64,
    pub signals_dropped: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BusHealth {
    /// Current queue depth. A number that only ever grows is a consumer that
    /// stopped, and that is the whole point of exposing it.
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

/// One row of the rooms table.
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
    /// Seconds since the room last had a player. `None` while it has players.
    pub empty_for_sec: Option<u64>,
    /// `live` | `empty` — derived, never stored, so it cannot disagree with
    /// `clients`.
    pub status: String,
    pub has_sim_sample: bool,
    pub sample_age_ms: Option<u64>,
    /// What a client dials to reach this room, port-qualified.
    pub signaling_url: String,
    pub server_id: String,
}

/// One row of the workers strip: one entry per plane, not per OS thread.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRow {
    /// `sim` | `control` | `transport`.
    pub name: String,
    /// `game` | `tokio` | `mio` — how it is scheduled, which is what decides
    /// whether a stall is a priority problem or a slow handler.
    pub kind: String,
    pub threads: u32,
    pub ccu: u32,
    /// Microseconds of the window's p50 tick, `None` for planes with no tick.
    pub tick_p50_us: Option<u64>,
    pub tick_p99_us: Option<u64>,
    pub nominal_tick_us: Option<u64>,
    pub overruns: Option<u64>,
    /// Deltas this plane accepted and dropped under backpressure.
    pub deltas_sent: Option<u64>,
    pub deltas_dropped: Option<u64>,
    pub queue_depth: Option<usize>,
    /// `None` until the sim publishes at least once.
    pub live: bool,
}

/// One row of the clients table.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientRow {
    /// The transport's own session id (the WS client id, the WebRTC peer addr,
    /// or the UDP source address).
    pub session: String,
    /// `websocket` | `webrtc` | `native-udp`.
    pub transport: String,
    pub room: String,
    /// The player id the host assigned, `None` for a spectator.
    pub player_id: Option<i64>,
    /// Highest input seq the server applied from this session — the same number
    /// the client sees echoed back as `ackedSeq` in the delta tail.
    pub acked_seq: Option<u32>,
    /// Milliseconds since this session's last input. `None` for a session that
    /// has never sent one, which is a connected-but-silent client.
    pub idle_ms: Option<u64>,
}

/// The graph payload: the rolling series plus the newest window's histogram.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickSeries {
    /// Seconds each sample spans (the sim publishes once per second).
    pub interval_sec: u64,
    pub points: Vec<TickPoint>,
    pub histogram: [u32; TICK_HISTOGRAM_BUCKETS],
    /// Upper edge of each bucket in µs, so the client can label the bars
    /// without redoing the base-2 arithmetic.
    pub bucket_edges_us: [u64; TICK_HISTOGRAM_BUCKETS],
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickPoint {
    /// Milliseconds since this plane's monotonic epoch, same epoch as
    /// `sampleAgeMs` so a client can compare them.
    pub at_ms: u64,
    pub p50_us: u64,
    pub p99_us: u64,
    pub hz: f64,
    pub players: u32,
    pub seq: u64,
    /// `true` when no sample reached this plane between two pushes, so a gap in
    /// the line is marked rather than drawn as a flat run.
    pub gap: bool,
}

/// Everything one `/ws/dashboard` message carries. Tagged so a client can
/// `switch` on `type` without guessing.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum WsFrame {
    /// The first frame after a socket opens: the current state, so a page load
    /// is never a blank screen waiting for a second.
    Hello {
        overview: Box<Overview>,
        ticks: Box<TickSeries>,
        protocol: u32,
    },
    /// One per second thereafter.
    Snapshot {
        overview: Box<Overview>,
        ticks: Box<TickSeries>,
    },
    /// This plane is closing the socket; `reason` is for the toast.
    Goodbye { reason: String },
}

/// `/ws/dashboard` protocol version. A client that does not speak it says so
/// instead of misreading a frame it has never seen.
pub const WS_PROTOCOL: u32 = 1;

/// One stored sample: the snapshot plus when this plane saw it.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub snapshot: StatusSnapshot,
    /// Monotonic ms at arrival, so a gap between samples is detectable.
    pub arrived_ms: u64,
}

/// The reader-side ring `status::pump` feeds and the dashboard handlers read.
///
/// Deliberately not in `Bus`: the sim owns its own history (tick rings,
/// percentiles) and this owns the *plane's* view of it, which is the only place
/// a gap between two publishes can be noticed.
pub struct DashboardHistory {
    inner: Mutex<VecDeque<HistoryEntry>>,
    epoch: Instant,
}

impl DashboardHistory {
    pub fn new() -> DashboardHistory {
        DashboardHistory { inner: Mutex::new(VecDeque::with_capacity(HISTORY_LEN)), epoch: Instant::now() }
    }

    /// Monotonic ms since this ring's epoch — the same clock `TickPoint::at_ms`
    /// speaks, so a chart and an `ageMs` never disagree by construction.
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Appends, dropping the oldest past [`HISTORY_LEN`]. A poisoned lock is
    /// still a usable ring, so this recovers rather than propagating a panic
    /// from an unrelated reader into the pump.
    pub fn push(&self, snapshot: StatusSnapshot, arrived_ms: u64) {
        let mut q = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if q.len() == HISTORY_LEN {
            q.pop_front();
        }
        q.push_back(HistoryEntry { snapshot, arrived_ms });
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// `true` when nothing has ever been published.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn newest(&self) -> Option<HistoryEntry> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).back().cloned()
    }

    /// Oldest-to-newest copy, for building a [`TickSeries`].
    pub fn snapshot_all(&self) -> Vec<HistoryEntry> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
    }

    /// Drops every sample. Only a test or a restart path wants this.
    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

impl Default for DashboardHistory {
    fn default() -> Self {
        DashboardHistory::new()
    }
}

/// Bucket edges in µs for the base-2 histogram: `[2, 4, 8, … , 4096]`, with the
/// last edge naming the open-ended top bucket.
pub fn bucket_edges_us() -> [u64; TICK_HISTOGRAM_BUCKETS] {
    let mut edges = [0u64; TICK_HISTOGRAM_BUCKETS];
    let mut i = 0;
    while i < TICK_HISTOGRAM_BUCKETS {
        edges[i] = 1u64 << (i + 1);
        i += 1;
    }
    edges
}

/// Adds one tick duration to a histogram in place: bucket `i` for
/// `[2^i, 2^(i+1))` µs, top bucket for anything longer.
pub fn histogram_add(hist: &mut [u32; TICK_HISTOGRAM_BUCKETS], micros: u64) {
    // floor(log2(micros)) is the bucket; `max(1)` keeps 0 out of the undefined
    // case, and an instant tick belongs in the first bucket anyway.
    let bucket = (63 - micros.max(1).leading_zeros()) as usize;
    hist[bucket.min(TICK_HISTOGRAM_BUCKETS - 1)] += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_buckets_by_power_of_two() {
        let mut h = [0u32; TICK_HISTOGRAM_BUCKETS];
        histogram_add(&mut h, 1); // [0,2) -> bucket 0
        histogram_add(&mut h, 3); // [2,4) -> bucket 1
        histogram_add(&mut h, 5_000); // over the top -> last bucket
        assert_eq!((h[0], h[1], h[TICK_HISTOGRAM_BUCKETS - 1]), (1, 1, 1));
        assert_eq!(h.iter().sum::<u32>(), 3);
    }

    #[test]
    fn bucket_edges_match_the_buckets() {
        let e = bucket_edges_us();
        assert_eq!(e[0], 2);
        assert_eq!(e[1], 4);
        assert_eq!(e[TICK_HISTOGRAM_BUCKETS - 1], 4096);
        assert_eq!(TICK_HISTOGRAM_BUCKETS, 12);
    }

    #[test]
    fn ring_keeps_the_newest_history_len_samples() {
        let h = DashboardHistory::new();
        for i in 0..(HISTORY_LEN + 50) {
            h.push(StatusSnapshot { seq: i as u64, ..Default::default() }, i as u64);
        }
        assert_eq!(h.len(), HISTORY_LEN);
        assert_eq!(h.newest().unwrap().snapshot.seq, (HISTORY_LEN + 49) as u64);
        // Oldest surviving entry is the one just after the evicted head.
        assert_eq!(h.snapshot_all().first().unwrap().snapshot.seq, 50);
    }

    /// Every group must serialise even when nothing has happened: a missing key
    /// is `undefined` in the browser, which is indistinguishable from a dead
    /// server.
    #[test]
    fn an_idle_overview_carries_every_key_the_dashboard_reads() {
        let json = serde_json::to_value(Overview::default()).unwrap();
        for path in [
            "/server/name",
            "/server/port",
            "/server/controlPort",
            "/server/webPort",
            "/uptimeSec",
            "/ccu",
            "/rooms",
            "/capacity/players",
            "/capacity/max",
            "/tick/hz",
            "/tick/configuredHz",
            "/tick/nominalUs",
            "/tick/p50Us",
            "/tick/p99Us",
            "/tick/overruns",
            "/tick/samples",
            "/tick/seq",
            "/tick/histogram",
            "/transport/websocket",
            "/transport/webrtc",
            "/transport/nativeUdp",
            "/transport/signalPeers",
            "/transport/signalsDelivered",
            "/transport/signalsDropped",
            "/bus/inputsQueued",
            "/bus/commandsQueued",
            "/bus/deltasQueued",
            "/bus/deltasSent",
            "/bus/deltasDroppedFull",
            "/bus/inputsDropped",
            "/seats/outstanding",
            "/seats/minted",
            "/seats/rejected",
            "/seats/refused",
            "/seats/bound",
            "/seats/ttlSecs",
            "/rateLimit/allowed",
            "/rateLimit/denied",
            "/rateLimit/squeezed",
            "/rateLimit/swept",
            "/rateLimit/tracked",
            "/rateLimit/rps",
            "/rateLimit/burst",
            "/agones",
            "/hasSimSample",
            "/sampleAgeMs",
        ] {
            assert!(json.pointer(path).is_some(), "missing {path} in {json}");
        }
        assert_eq!(json["tick"]["histogram"].as_array().unwrap().len(), TICK_HISTOGRAM_BUCKETS);
        assert_eq!(json["hasSimSample"].as_bool(), Some(false));
    }

    #[test]
    fn a_ws_frame_is_self_describing() {
        let f = WsFrame::Hello {
            overview: Box::new(Overview::default()),
            ticks: Box::new(TickSeries::default()),
            protocol: WS_PROTOCOL,
        };
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["type"].as_str(), Some("hello"));
        assert_eq!(json["protocol"].as_u64(), Some(1));
        assert!(json["overview"].is_object());
        let s = serde_json::to_value(WsFrame::Goodbye { reason: "plane shutting down".into() }).unwrap();
        assert_eq!(s["type"].as_str(), Some("goodbye"));
    }

    /// A `TickSeries` with no samples must still name its buckets, or the bar
    /// chart renders an empty axis with no scale.
    #[test]
    fn an_empty_series_still_ships_its_axis() {
        let json = serde_json::to_value(TickSeries::default()).unwrap();
        assert!(json["points"].as_array().unwrap().is_empty());
        assert_eq!(json["bucketEdgesUs"].as_array().unwrap().len(), TICK_HISTOGRAM_BUCKETS);
    }
}
