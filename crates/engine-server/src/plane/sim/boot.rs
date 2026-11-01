use std::sync::Arc;

use engine_bus::Bus;
use engine_physics::PhysicsWorld3D;
use engine_wasm::WasmRuntime;
use mio::Poll;

use crate::plane::{self, PlaneConfig};
use crate::rooms::{self, GameRoom};

pub(super) fn boot(
    bus: Arc<Bus>,
    cfg: &PlaneConfig,
) -> Option<(
    plane::udp::UdpPlane,
    Poll,
    GameRoom,
    Option<engine_wasm::EpochTicker>,
)> {
    let seats = Arc::new(plane::SeatRegistry::new(&cfg.seat_secret));
    let mut udp = match plane::udp::UdpPlane::new(
        &cfg.udp_addr,
        Arc::clone(&bus),
        Arc::clone(&seats),
        cfg.lifecycle.peer_idle(),
    ) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(
                "[sim] ✖️ UDP plane bind {} failed: {e} — sim thread exiting",
                cfg.udp_addr
            );
            return None;
        }
    };
    let poll = match Poll::new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[sim] ✖️ mio poll creation failed: {e} — sim thread exiting");
            return None;
        }
    };
    if let Err(e) = udp.register(&poll) {
        tracing::error!("[sim] ✖️ UDP socket registration failed: {e} — sim thread exiting");
        return None;
    }
    udp.set_tick_hz(cfg.tick_hz);

    let watchdog = cfg.watchdog.clone();
    let runtime = match WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        enable_epoch_interruption: watchdog.enabled,
        epoch_deadline_ticks: watchdog.enabled.then_some(watchdog.epoch_deadline_ticks),
        ..engine_wasm::WasmConfig::default()
    }) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[sim] ✖️ wasm runtime init failed: {e} — sim thread exiting");
            return None;
        }
    };
    let _epoch_ticker = watchdog
        .enabled
        .then(|| runtime.spawn_epoch_ticker(watchdog.epoch_period()));
    let module = match runtime.load_or_compile_cached(&cfg.wasm_path) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("[sim] ✖️ wasm module load failed: {e} — sim thread exiting");
            return None;
        }
    };
    let linker = rooms::build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
    let pre = match runtime.prepare(&linker, &module) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[sim] ✖️ pre-instantiation failed: {e} — sim thread exiting");
            return None;
        }
    };
    let physics = std::fs::read_to_string("arena_map.json")
        .ok()
        .and_then(|json| PhysicsWorld3D::from_json(&json).ok())
        .unwrap_or_default();
    let room = match GameRoom::new(&pre, cfg.max_players, physics) {
        Ok(room) => room,
        Err(e) => {
            tracing::error!("[sim] ✖️ room instantiation failed: {e} — sim thread exiting");
            return None;
        }
    };
    Some((udp, poll, room, _epoch_ticker))
}
