use std::time::Duration;

use engine_bus::Bus;

use crate::rooms::GameServer;

pub(crate) const REHYDRATE_WAIT: Duration = Duration::from_millis(750);

pub(crate) fn rehydrate_persistent_rooms(
    server: &mut GameServer,
    bus: &Bus,
    cfg: &engine_config::GameConfig,
    wait: Duration,
) {
    if !cfg.permissions.storage {
        return;
    }
    let mut names: Vec<&str> = cfg
        .lifecycle
        .persistent_rooms
        .iter()
        .map(String::as_str)
        .collect();
    if !names.contains(&cfg.name.as_str()) {
        names.push(cfg.name.as_str());
    }
    for name in names {
        let Some(saved) = bus.fetch_snapshot(name, wait) else {
            continue;
        };
        tracing::info!(
            "[server] 🧊 rehydrating room '{name}' from the datastore ({} bytes, snapshot {}, saved by a {} backend)",
            saved.bytes.len(),
            saved.seq,
            saved.backend
        );
        server.save_snapshot(name, saved.bytes);
    }
}
