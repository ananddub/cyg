use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, table: &toml::value::Table) {
    if let Some(a) = table.get("aoi").and_then(|v| v.as_table()) {
        cfg.aoi.enabled = a.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        if let Some(v) = a
            .get("near_radius")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        {
            cfg.aoi.near_radius = v;
        }
        if let Some(v) = a
            .get("far_radius")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        {
            cfg.aoi.far_radius = v;
        }
        if let Some(v) = a.get("skip_ticks").and_then(|v| v.as_integer()) {
            cfg.aoi.skip_ticks = v.max(1) as u64;
        }
        if let Some(v) = a
            .get("bullet_radius")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        {
            cfg.aoi.bullet_radius = v;
        }
    }

    if let Some(r) = table.get("rollback").and_then(|v| v.as_table()) {
        cfg.rollback.enabled = r.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        if let Some(v) = r.get("max_replay_ticks").and_then(|v| v.as_integer()) {
            cfg.rollback.max_replay_ticks = v.max(1) as u64;
        }
        if let Some(v) = r.get("journal_size").and_then(|v| v.as_integer()) {
            cfg.rollback.journal_size = v.max(8) as usize;
        }
    }

    if let Some(l) = table.get("lifecycle").and_then(|v| v.as_table()) {
        if let Some(v) = l.get("room_max_seconds").and_then(|v| v.as_integer()) {
            cfg.lifecycle.room_max_seconds = v.max(0) as u64;
        }
        if let Some(v) = l.get("reap_idle_secs").and_then(|v| v.as_integer()) {
            cfg.lifecycle.reap_idle_secs = v.max(0) as u64;
        }
        if let Some(v) = l.get("peer_idle_ms").and_then(|v| v.as_integer()) {
            cfg.lifecycle.peer_idle_ms = v.max(0) as u64;
        }
        if let Some(v) = l.get("seat_hold_secs").and_then(|v| v.as_integer()) {
            cfg.lifecycle.seat_hold_secs = v.max(0) as u64;
        }
        if let Some(rooms) = l.get("persistent_rooms").and_then(|v| v.as_array()) {
            cfg.lifecycle.persistent_rooms = rooms
                .iter()
                .filter_map(|room| room.as_str())
                .map(str::to_owned)
                .collect();
        }
        if let Some(v) = l.get("drain_grace_secs").and_then(|v| v.as_integer()) {
            cfg.lifecycle.drain_grace_secs = v.max(0) as u64;
        }
    }

    if let Some(w) = table.get("watchdog").and_then(|v| v.as_table()) {
        if let Some(v) = w.get("enabled").and_then(|v| v.as_bool()) {
            cfg.watchdog.enabled = v;
        }
        if let Some(v) = w.get("tick_budget_ms").and_then(|v| v.as_integer()) {
            cfg.watchdog.tick_budget_ms = v.max(0) as u64;
        }
        if let Some(v) = w.get("epoch_deadline_ticks").and_then(|v| v.as_integer()) {
            cfg.watchdog.epoch_deadline_ticks = v.max(0) as u64;
        }
        if let Some(v) = w.get("max_ticks_per_frame").and_then(|v| v.as_integer()) {
            cfg.watchdog.max_ticks_per_frame = v.max(0) as u32;
        }
    }

    if let Some(g) = table.get("gameplay").and_then(|v| v.as_table()) {
        if let Some(events) = g.get("record_events").and_then(|v| v.as_array()) {
            cfg.gameplay.record_events = events
                .iter()
                .filter_map(|event| event.as_str())
                .map(str::to_owned)
                .collect();
        }
        if let Some(v) = g.get("event_ttl_days").and_then(|v| v.as_integer()) {
            cfg.gameplay.event_ttl_days = v.max(0) as u32;
        }
        if let Some(v) = g.get("match_history_ttl_days").and_then(|v| v.as_integer()) {
            cfg.gameplay.match_history_ttl_days = v.max(0) as u32;
        }
        if let Some(v) = g.get("result_event").and_then(|v| v.as_str()) {
            cfg.gameplay.result_event = v.to_string();
        }
    }
}
