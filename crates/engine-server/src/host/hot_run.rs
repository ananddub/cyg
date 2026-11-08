use std::time::{Duration, Instant};

use crate::host::*;

impl<'a> HotReload<'a> {
    pub(crate) fn run(
        &mut self,
        server: &mut GameServer,
        bus: &Arc<Bus>,
        cfg: &GameConfig,
        tick_interval: &mut Duration,
    ) {
        if self.last_wasm_check.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_wasm_check = Instant::now();

        let mut latest_reload: Option<engine_bus::WasmReload> = None;
        while let Ok(reload) = bus.reloads_rx.try_recv() {
            tracing::info!(
                "[server] 📦 console wasm reload: game '{}' {} v{} (sha256 {}…)",
                reload.game_key,
                match reload.kind {
                    engine_bus::ReloadKind::Upload => "uploaded",
                    engine_bus::ReloadKind::Rollback => "rolled back to",
                },
                reload.version,
                &reload.sha256[..reload.sha256.len().min(12)]
            );
            latest_reload = Some(reload);
        }
        if let Some(reload) = latest_reload {
            match prepare_console_module(
                &*self.runtime,
                &reload.sha256,
                &reload.bytes,
                cfg.permissions,
                &cfg.features,
            ) {
                Ok(pre) => {
                    self.console_active = true;
                    server.hot_reload(pre);
                    tracing::info!(
                        "[server] 🔥 console module v{} swapped into live rooms",
                        reload.version
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "[server] ⚠️ console reload v{} rejected: {e}",
                        reload.version
                    );
                }
            }
        }

        while let Ok(settings) = bus.settings_rx.try_recv() {
            let rate = settings.tick_rate.max(1);
            *tick_interval = Duration::from_secs_f64(1.0 / rate as f64);
            tracing::info!(
                "[server] ⚙️  console setting applied: tick rate now {rate} Hz (was {} Hz)",
                cfg.tick_rate
            );
        }

        while let Ok(map) = bus.map_reloads_rx.try_recv() {
            match engine_config::MapFormat::parse(&map.format) {
                Some(engine_config::MapFormat::Map2d) => {
                    match serde_json::from_slice::<engine_physics::MapConfig2D>(&map.bytes) {
                        Ok(m) => {
                            server.arena_2d = Some(m);
                            tracing::info!(
                                "[server] 🗺️  console map '{}' v{} (map2d) active — rooms spawned from now on use it",
                                map.name,
                                map.version
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "[server] failed to parse reloaded map2d '{}': {e}",
                                map.name
                            );
                        }
                    }
                }
                _ => {
                    tracing::info!(
                        "[server] 🗺️  console map '{}' v{} ({}) stored — applies at next boot",
                        map.name,
                        map.version,
                        map.format
                    );
                }
            }
        }

        self.detect_rebuild(server, cfg);
    }
}
