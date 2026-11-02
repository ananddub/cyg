use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
    match key {
        "room_max_seconds" => {
            cfg.lifecycle.room_max_seconds = value.parse().unwrap_or(cfg.lifecycle.room_max_seconds)
        }
        "reap_idle_secs" => {
            cfg.lifecycle.reap_idle_secs = value.parse().unwrap_or(cfg.lifecycle.reap_idle_secs)
        }
        "peer_idle_ms" => {
            cfg.lifecycle.peer_idle_ms = value.parse().unwrap_or(cfg.lifecycle.peer_idle_ms)
        }
        "seat_hold_secs" => {
            cfg.lifecycle.seat_hold_secs = value.parse().unwrap_or(cfg.lifecycle.seat_hold_secs)
        }
        "persistent_rooms" => {
            cfg.lifecycle.persistent_rooms = crate::social::split_ids(value);
        }
        "drain_grace_secs" => {
            cfg.lifecycle.drain_grace_secs = value.parse().unwrap_or(cfg.lifecycle.drain_grace_secs)
        }
        _ => {}
    }
}
