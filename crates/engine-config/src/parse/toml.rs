use crate::game::{GameConfig, Runtime};

/// `[control.quotas]` keys that configure the limiter rather than name a scope.
const QUOTA_KEYS: &[&str] = &[
    "enabled",
    "exempt_console",
    "player_rps",
    "player_burst",
    "default_cap",
    "default_window_secs",
];

pub(crate) fn fill(cfg: &mut GameConfig, table: &toml::value::Table) {
    let int = |k: &str| table.get(k).and_then(|v| v.as_integer());
    let float = |k: &str| {
        table
            .get(k)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
    };
    let boolean = |k: &str| table.get(k).and_then(|v| v.as_bool());
    let string = |k: &str| table.get(k).and_then(|v| v.as_str()).map(str::to_string);

    if let Some(v) = string("name") {
        cfg.name = v;
    }
    if let Some(v) = string("game_id") {
        cfg.game_id = Some(v);
    }
    if let Some(v) = string("game_key") {
        cfg.game_key = Some(v);
    }
    if let Some(v) = string("runtime")
        && let Some(r) = Runtime::parse(&v)
    {
        cfg.runtime = r;
    }
    if let Some(v) = string("source") {
        cfg.source = v.into();
    }
    if let Some(v) = string("out") {
        cfg.out = v.into();
    }
    if let Some(v) = string("sdk_dir") {
        cfg.sdk_dir = v.into();
    }
    if let Some(v) = string("website") {
        cfg.website = v.into();
    }
    if let Some(v) = int("port") {
        cfg.port = v as u16;
    }
    if let Some(v) = int("quic_port") {
        cfg.quic_port = v as u16;
    }
    if let Some(v) = int("web_port") {
        cfg.web_port = v as u16;
    }
    if let Some(v) = int("tick_rate") {
        cfg.tick_rate = v as u32;
    }
    if let Some(v) = int("max_clients") {
        cfg.max_clients = v as i32;
    }
    if let Some(v) = int("max_rooms") {
        cfg.max_rooms = v as u32;
    }
    if let Some(v) = float("width") {
        cfg.width = v;
    }
    if let Some(v) = float("height") {
        cfg.height = v;
    }
    if let Some(v) = float("speed") {
        cfg.speed = v;
    }
    if let Some(v) = int("abi_version") {
        cfg.abi_version = v as u32;
    }
    if let Some(v) = boolean("allow_physics") {
        cfg.permissions.physics = v;
    }
    if let Some(v) = boolean("allow_lag_compensation") {
        cfg.permissions.lag_compensation = v;
    }
    if let Some(v) = boolean("allow_logging") {
        cfg.permissions.logging = v;
    }
    if let Some(v) = boolean("allow_storage") {
        cfg.permissions.storage = v;
    }
    if let Some(v) = boolean("allow_messaging") {
        cfg.permissions.messaging = v;
    }
    if let Some(v) = table.get("voice_ice_servers") {
        cfg.voice_ice_servers = v.to_string();
    }

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

    if let Some(c) = table.get("control").and_then(|v| v.as_table()) {
        let num = |k: &str| {
            c.get(k)
                .and_then(|v| v.as_integer())
                .map(|v| v.max(0) as u64)
        };
        if let Some(v) = c.get("enabled").and_then(|v| v.as_bool()) {
            cfg.control.enabled = v;
        }
        if let Some(v) = c.get("bind_addr").and_then(|v| v.as_str()) {
            cfg.control.bind_addr = v.to_string();
        }
        if let Some(v) = num("api_port") {
            cfg.control.api_port = v as u16;
        }
        if let Some(v) = num("workers").filter(|v| *v > 0) {
            cfg.control.workers = v as usize;
        }
        if let Some(v) = num("seat_ttl_secs").filter(|v| *v > 0) {
            cfg.control.seat_ttl_secs = v;
        }
        if let Some(v) = c.get("seat_secret").and_then(|v| v.as_str()) {
            cfg.control.seat_secret = v.to_string();
        }
        if let Some(v) = num("rate_limit_rps").filter(|v| *v > 0) {
            cfg.control.rate_limit_rps = v as u32;
        }
        if let Some(v) = num("rate_limit_burst").filter(|v| *v > 0) {
            cfg.control.rate_limit_burst = v as u32;
        }
        if let Some(a) = c.get("agones").and_then(|v| v.as_table()) {
            if let Some(v) = a.get("enabled").and_then(|v| v.as_bool()) {
                cfg.control.agones.enabled = v;
            }
            if let Some(v) = a
                .get("http_port")
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
            {
                cfg.control.agones.http_port = v as u16;
            }
            if let Some(v) = a
                .get("health_interval_ms")
                .and_then(|v| v.as_integer())
                .filter(|v| *v >= 200)
            {
                cfg.control.agones.health_interval_ms = v as u64;
            }
            if let Some(v) = a.get("host").and_then(|v| v.as_str()) {
                cfg.control.agones.host = v.to_string();
            }
        }
        if let Some(s) = c.get("session").and_then(|v| v.as_table()) {
            let sec = |k: &str| {
                s.get(k)
                    .and_then(|v| v.as_integer())
                    .filter(|v| *v > 0)
                    .map(|v| v as i64)
            };
            // `database_url` is the pre-SurrealDB name; still accepted so an
            // existing project config keeps booting.
            if let Some(v) = s
                .get("database_endpoint")
                .or_else(|| s.get("database_url"))
                .and_then(|v| v.as_str())
            {
                cfg.control.session.database_endpoint = v.to_string();
            }
            for (key, field) in [
                ("database_ns", &mut cfg.control.session.database_ns),
                ("database_db", &mut cfg.control.session.database_db),
                ("database_user", &mut cfg.control.session.database_user),
                ("database_pass", &mut cfg.control.session.database_pass),
                ("blobs", &mut cfg.control.session.blobs),
            ] {
                if let Some(v) = s.get(key).and_then(|v| v.as_str()) {
                    *field = v.to_string();
                }
            }
            for (key, field) in [
                (
                    "database_memtable_mib",
                    &mut cfg.control.session.database_memtable_mib,
                ),
                (
                    "database_cache_mib",
                    &mut cfg.control.session.database_cache_mib,
                ),
            ] {
                if let Some(v) = s.get(key).and_then(|v| v.as_integer()) {
                    *field = v.max(0) as u64;
                }
            }
            if let Some(v) = s.get("signing_key").and_then(|v| v.as_str()) {
                cfg.control.session.signing_key = v.to_string();
            }
            if let Some(v) = s.get("refresh_signing_key").and_then(|v| v.as_str()) {
                cfg.control.session.refresh_signing_key = v.to_string();
            }
            if let Some(v) = s.get("server_key").and_then(|v| v.as_str()) {
                cfg.control.session.server_key = v.to_string();
            }
            if let Some(v) = sec("token_expires_sec") {
                cfg.control.session.token_expiry_secs = v;
            }
            if let Some(v) = sec("refresh_token_expires_sec") {
                cfg.control.session.refresh_expiry_secs = v;
            }
        }
        if let Some(cn) = c.get("console").and_then(|v| v.as_table()) {
            if let Some(v) = cn.get("username").and_then(|v| v.as_str()) {
                cfg.control.console.username = v.to_string();
            }
            if let Some(v) = cn.get("password").and_then(|v| v.as_str()) {
                cfg.control.console.password = v.to_string();
            }
            if let Some(v) = cn.get("signing_key").and_then(|v| v.as_str()) {
                cfg.control.console.signing_key = v.to_string();
            }
            if let Some(v) = cn
                .get("token_expires_sec")
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
            {
                cfg.control.console.token_expiry_secs = v as i64;
            }
        }
        if let Some(p) = c.get("iap").and_then(|v| v.as_table()) {
            for (key, field) in [
                (
                    "apple_shared_secret",
                    &mut cfg.control.iap.apple_shared_secret,
                ),
                ("apple_verify_url", &mut cfg.control.iap.apple_verify_url),
                ("apple_sandbox_url", &mut cfg.control.iap.apple_sandbox_url),
                (
                    "google_service_account",
                    &mut cfg.control.iap.google_service_account,
                ),
                (
                    "google_package_name",
                    &mut cfg.control.iap.google_package_name,
                ),
                ("google_token_url", &mut cfg.control.iap.google_token_url),
                ("google_api_base", &mut cfg.control.iap.google_api_base),
            ] {
                if let Some(v) = p.get(key).and_then(|v| v.as_str()) {
                    *field = v.to_string();
                }
            }
            if let Some(v) = p.get("allow_unverified").and_then(|v| v.as_bool()) {
                cfg.control.iap.allow_unverified = v;
            }
        }
        if let Some(s) = c.get("social").and_then(|v| v.as_table()) {
            if let Some(v) = s.get("google_client_ids") {
                if let Some(ids) = v.as_array() {
                    cfg.control.social.google_client_ids = ids
                        .iter()
                        .filter_map(|id| id.as_str())
                        .map(str::to_owned)
                        .collect();
                } else if let Some(ids) = v.as_str() {
                    cfg.control.social.google_client_ids = crate::social::split_ids(ids);
                }
            }
            for (key, field) in [
                (
                    "google_tokeninfo_url",
                    &mut cfg.control.social.google_tokeninfo_url,
                ),
                ("apple_client_id", &mut cfg.control.social.apple_client_id),
                ("apple_keys_url", &mut cfg.control.social.apple_keys_url),
                ("facebook_app_id", &mut cfg.control.social.facebook_app_id),
                (
                    "facebook_app_secret",
                    &mut cfg.control.social.facebook_app_secret,
                ),
                (
                    "facebook_graph_url",
                    &mut cfg.control.social.facebook_graph_url,
                ),
                (
                    "steam_web_api_key",
                    &mut cfg.control.social.steam_web_api_key,
                ),
                ("steam_api_url", &mut cfg.control.social.steam_api_url),
            ] {
                if let Some(v) = s.get(key).and_then(|v| v.as_str()) {
                    *field = v.to_string();
                }
            }
            if let Some(v) = s
                .get("steam_app_id")
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
            {
                cfg.control.social.steam_app_id = v as u32;
            }
            if let Some(v) = s.get("allow_unverified").and_then(|v| v.as_bool()) {
                cfg.control.social.allow_unverified = v;
            }
        }
        if let Some(q) = c.get("quotas").and_then(|v| v.as_table()) {
            if let Some(v) = q.get("enabled").and_then(|v| v.as_bool()) {
                cfg.control.quotas.enabled = v;
            }
            if let Some(v) = q.get("exempt_console").and_then(|v| v.as_bool()) {
                cfg.control.quotas.exempt_console = v;
            }
            for (key, field) in [
                ("player_rps", &mut cfg.control.quotas.player_rps),
                ("player_burst", &mut cfg.control.quotas.player_burst),
            ] {
                if let Some(v) = q.get(key).and_then(|v| v.as_integer()).filter(|v| *v > 0) {
                    *field = v as u32;
                }
            }
            if let Some(v) = q.get("default_cap").and_then(|v| v.as_integer()) {
                cfg.control.quotas.fallback.cap = v.max(0) as u32;
            }
            if let Some(v) = q
                .get("default_window_secs")
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
            {
                cfg.control.quotas.fallback.period_secs = v as u64;
            }
            // Any other key names a charged scope: an integer is its cap and
            // keeps that scope's window, a table sets both.
            for (name, value) in q {
                if QUOTA_KEYS.contains(&name.as_str()) {
                    continue;
                }
                let mut scope = cfg.control.quotas.window(name);
                if let Some(cap) = value.as_integer() {
                    scope.cap = cap.max(0) as u32;
                } else if let Some(t) = value.as_table() {
                    if let Some(cap) = t.get("cap").and_then(|v| v.as_integer()) {
                        scope.cap = cap.max(0) as u32;
                    }
                    if let Some(secs) = t
                        .get("period_secs")
                        .and_then(|v| v.as_integer())
                        .filter(|v| *v > 0)
                    {
                        scope.period_secs = secs as u64;
                    }
                } else {
                    continue;
                }
                cfg.control.quotas.scopes.insert(name.clone(), scope);
            }
        }
    }
}
