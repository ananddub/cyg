use crate::game::{GameConfig, Runtime};

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
    if let Some(v) = int("web_port") {
        cfg.web_port = v as u16;
    }
    if let Some(v) = int("tick_rate") {
        cfg.tick_rate = v as u32;
    }
    if let Some(v) = int("max_clients") {
        cfg.max_clients = v as i32;
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
            if let Some(v) = s.get("database_url").and_then(|v| v.as_str()) {
                cfg.control.session.database_url = v.to_string();
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
            if let Some(v) = s
                .get("max_db_connections")
                .and_then(|v| v.as_integer())
                .filter(|v| *v > 0)
            {
                cfg.control.session.max_db_connections = v as u32;
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
    }
}
