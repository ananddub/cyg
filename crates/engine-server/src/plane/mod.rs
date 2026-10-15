//! # The two-plane thread topology (moved verbatim from `src/threads/`)
//!
//! `sim` is the pinned MIO OS thread: raw UDP :7778 + the 128 Hz wasm
//! simulation. Everything crypto/async belongs to the Tokio plane and is out
//! of scope here. This module owns spawning, core pinning, and the clock
//! helpers every plane file shares.
//!
//! NOTE: `threads/aoi.rs` was NOT moved here — agent A7 is folding it into
//! `engine-spatial`. `threads/status.rs` likewise goes to `engine-control`
//! (agent A3). Call sites in [`sim`] point at those destination crates and
//! are re-pointed at integration.

pub mod dispatch;
pub mod pin;
pub mod reliable;
pub mod seats;
pub mod sim;
pub mod udp;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_bus::Bus;

// Re-exports: names the root binary referenced as `crate::threads::X`.
pub use crate::plane::dispatch::{emit_delta, wrap_body};
pub use crate::plane::pin::{current_core, pin_current_thread};
pub use crate::plane::reliable::{QueueError, ReliableCounts, ReliableLane};
pub use crate::plane::seats::{SeatGrant, SeatRegistry};
pub use crate::plane::udp::UdpPlane;

/// Monotonic nanoseconds since an arbitrary process-wide epoch. Used for
/// kernel→sim latency: stamp at datagram read, compare at queue drain.
pub fn mono_ns() -> u64 {
    static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    EPOCH.elapsed().as_nanos() as u64
}

/// Runtime knobs for the sim plane, env-driven so the existing WS flow (and
/// other agents' phases) is never disturbed by these files.
///
/// * `OX_PLANES=1`          — enable the raw UDP + sim plane at all
/// * `OX_SIM_CORE=2`        — core to pin the sim thread to
/// * `OX_UDP_ADDR=0.0.0.0:7778` — raw UDP bind address
/// * `OX_TICK_HZ=128`       — simulation tick rate
/// * `OX_SEAT_SECRET`       — HMAC key for seat tokens (required)
/// * `OX_WASM_PATH`         — compiled room wasm; defaults to the project's
///                            `ox-server/room_logic.wasm` next to `ox.toml`
#[derive(Clone, Debug)]
pub struct PlaneConfig {
    pub enabled: bool,
    pub sim_core: usize,
    pub udp_addr: String,
    pub tick_hz: f64,
    pub seat_secret: String,
    pub wasm_path: PathBuf,
    pub room_name: String,
    pub max_players: i32,
}

impl PlaneConfig {
    pub fn from_env(project_dir: &std::path::Path) -> PlaneConfig {
        let enabled = std::env::var("OX_PLANES").map_or(false, |v| v == "1");
        let sim_core = std::env::var("OX_SIM_CORE").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
        let udp_addr = std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into());
        let tick_hz = std::env::var("OX_TICK_HZ").ok().and_then(|v| v.parse().ok()).unwrap_or(128.0);
        let seat_secret =
            std::env::var("OX_SEAT_SECRET").unwrap_or_else(|_| engine_config::DEV_SEAT_SECRET.into());
        let wasm_path = std::env::var("OX_WASM_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| project_dir.join("ox-server").join("room_logic.wasm"));
        let room_name = std::env::var("OX_ROOM").unwrap_or_else(|_| "tank-3d".into());
        let max_players = std::env::var("OX_MAX_PLAYERS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
        PlaneConfig { enabled, sim_core, udp_addr, tick_hz, seat_secret, wasm_path, room_name, max_players }
    }
}

/// Spawn the pinned sim thread (raw UDP + 128 Hz wasm sim). The browser-side
/// consumer of `bus.deltas_rx` lives on the Tokio plane (Agent 2) and is not
/// started here.
pub fn spawn_sim_plane(bus: Arc<Bus>, cfg: PlaneConfig) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("ox-sim".into())
        .spawn(move || {
            if let Err(e) = pin::pin_current_thread(cfg.sim_core) {
                eprintln!("[threads] ⚠️ core pinning to {} failed: {e} (contin unpinned)", cfg.sim_core);
            }
            sim::run(bus, cfg);
        })
}

/// Nominal tick period for `tick_hz`.
pub fn tick_period(tick_hz: f64) -> Duration {
    Duration::from_nanos((1e9 / tick_hz.max(1.0)) as u64)
}
