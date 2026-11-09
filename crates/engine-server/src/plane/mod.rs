pub mod dispatch;
pub mod pin;
pub mod reliable;
pub mod seats;
pub mod sim;
pub mod udp;

use std::path::PathBuf;
use std::sync::Arc;

use engine_bus::Bus;

pub use crate::plane::dispatch::{emit_delta, wrap_body};
pub use crate::plane::pin::{current_core, pin_current_thread};
pub use crate::plane::reliable::{QueueError, ReliableCounts, ReliableLane};
pub use crate::plane::seats::{SeatGrant, SeatRegistry};
pub use crate::plane::udp::UdpPlane;
pub use engine_bus::clock::{mono_ns, tick_period};

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
    pub watchdog: engine_config::WatchdogConfig,
    pub lifecycle: engine_config::LifecycleConfig,
}

impl PlaneConfig {
    pub fn from_env(project_dir: &std::path::Path) -> PlaneConfig {
        let enabled = std::env::var("CYG_PLANES").map_or(false, |v| v == "1");
        let sim_core = std::env::var("CYG_SIM_CORE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);
        let udp_addr = std::env::var("CYG_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into());
        let tick_hz = std::env::var("CYG_TICK_HZ")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128.0);
        let seat_secret = std::env::var("CYG_SEAT_SECRET")
            .unwrap_or_else(|_| engine_config::DEV_SEAT_SECRET.into());
        let wasm_path = std::env::var("CYG_WASM_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| project_dir.join("ox-server").join("room_logic.wasm"));
        let room_name = std::env::var("CYG_ROOM").unwrap_or_else(|_| "tank-3d".into());
        let max_players = std::env::var("CYG_MAX_PLAYERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        let watchdog = engine_config::WatchdogConfig::from_env();
        let lifecycle = engine_config::LifecycleConfig::from_env();
        PlaneConfig {
            enabled,
            sim_core,
            udp_addr,
            tick_hz,
            seat_secret,
            wasm_path,
            room_name,
            max_players,
            watchdog,
            lifecycle,
        }
    }
}

pub fn spawn_sim_plane(
    bus: Arc<Bus>,
    cfg: PlaneConfig,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("ox-sim".into())
        .spawn(move || {
            if let Err(e) = pin::pin_current_thread(cfg.sim_core) {
                tracing::error!(
                    "[sim] ⚠️ core pinning to {} failed: {e} (continuing unpinned)",
                    cfg.sim_core
                );
            }
            sim::run(bus, cfg);
        })
}
