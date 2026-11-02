mod monetization;
mod session;
mod social;

use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, table: &toml::value::Table) {
    let Some(c) = table.get("control").and_then(|v| v.as_table()) else {
        return;
    };
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

    session::apply(cfg, c);
    social::apply(cfg, c);
    monetization::apply(cfg, c);
}
