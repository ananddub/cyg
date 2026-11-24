use serde::Serialize;

pub const TICK_HISTOGRAM_BUCKETS: usize = 12;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTelemetry {
    pub session: String,
    pub transport: String,
    pub player_id: Option<i64>,

    pub acked_seq: Option<u32>,

    pub idle_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoomPhase {
    Created,

    #[default]
    Active,

    Hibernating,

    Closed,
}

impl RoomPhase {
    pub fn allows(self, next: Self) -> bool {
        match self {
            RoomPhase::Created => matches!(next, RoomPhase::Active | RoomPhase::Closed),
            RoomPhase::Active => matches!(next, RoomPhase::Hibernating | RoomPhase::Closed),
            RoomPhase::Hibernating => matches!(next, RoomPhase::Active | RoomPhase::Closed),
            RoomPhase::Closed => false,
        }
    }
}

/// Lifecycle phase of the game-server process/host itself.
///
/// Sits next to [`RoomPhase`] so the status snapshot can carry both: a room
/// phase answers "is this room simulating?", this answers "is the whole
/// process booting, serving, draining, or gone?". The state machine that
/// owns transitions lives in `engine-manager`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerPhase {
    /// Constructing the runtime, lanes, and rooms. Not yet accepting players.
    #[default]
    Booting,
    /// Fully booted and in the allocation pool, accepting players.
    Ready,
    /// The cluster (e.g. Agones) handed this server a game; still serving.
    Allocated,
    /// Shutdown was requested; rooms get a grace window to finish.
    Draining,
    /// Drain finished; the process is on its way out.
    Shutdown,
}

impl ServerPhase {
    /// Whether this phase may transition to `next`.
    ///
    /// `Shutdown` is terminal. `Draining` only advances to `Shutdown`; it
    /// never goes back to `Ready`/`Allocated`, because a drain request is
    /// latched by the bus and cannot be un-requested.
    pub fn allows(self, next: Self) -> bool {
        match self {
            ServerPhase::Booting => matches!(
                next,
                ServerPhase::Ready | ServerPhase::Draining | ServerPhase::Shutdown
            ),
            ServerPhase::Ready => matches!(
                next,
                ServerPhase::Allocated | ServerPhase::Draining | ServerPhase::Shutdown
            ),
            ServerPhase::Allocated => matches!(
                next,
                ServerPhase::Ready | ServerPhase::Draining | ServerPhase::Shutdown
            ),
            ServerPhase::Draining => matches!(next, ServerPhase::Shutdown),
            ServerPhase::Shutdown => false,
        }
    }

    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            ServerPhase::Booting => "booting",
            ServerPhase::Ready => "ready",
            ServerPhase::Allocated => "allocated",
            ServerPhase::Draining => "draining",
            ServerPhase::Shutdown => "shutdown",
        }
    }

    #[inline]
    pub fn display(self) -> &'static str {
        self.as_str()
    }

    #[inline]
    pub fn is_serving(self) -> bool {
        matches!(self, ServerPhase::Ready | ServerPhase::Allocated)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomTelemetry {
    pub name: String,
    #[serde(default)]
    pub room_id: String,
    pub phase: RoomPhase,
    pub clients: u32,
    pub spectators: u32,
    pub tick: u64,

    pub age_sec: u64,

    pub closes_in_sec: Option<u64>,
    #[serde(default)]
    pub max_clients: u32,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StatusSnapshot {
    pub tick_hz_est: f64,
    pub tick_budget_p50_us: u64,
    pub tick_budget_p99_us: u64,
    pub tick_overruns: u64,

    pub desyncs: u64,
    pub nominal_tick_us: u64,

    pub stage_sim_us: u64,

    pub stage_guest_us: u64,

    pub tick_samples: u32,

    pub tick_histogram: [u32; TICK_HISTOGRAM_BUCKETS],
    pub kernel_to_sim_p50_us: u64,
    pub kernel_to_sim_p99_us: u64,
    pub inputs_dropped: u64,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub players: u32,
    pub empty_for_sec: u64,

    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,
    pub sessions: Vec<SessionTelemetry>,

    pub rooms: Vec<RoomTelemetry>,

    pub rooms_created: u32,
    pub rooms_active: u32,
    pub rooms_hibernating: u32,

    pub server_phase: ServerPhase,

    pub seq: u64,

    pub updated_ms: u64,
}

#[cfg(test)]
mod tests;
