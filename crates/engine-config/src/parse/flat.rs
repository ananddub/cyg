use crate::game::{GameConfig, Runtime};

pub(crate) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
    match key {
        "name" => cfg.name = value.to_string(),
        "runtime" => {
            if let Some(r) = Runtime::parse(value) {
                cfg.runtime = r;
            }
        }
        "source" => cfg.source = value.into(),
        "out" => cfg.out = value.into(),
        "sdk_dir" => cfg.sdk_dir = value.into(),
        "website" => cfg.website = value.into(),
        "port" => cfg.port = value.parse().unwrap_or(cfg.port),
        "web_port" => cfg.web_port = value.parse().unwrap_or(cfg.web_port),
        "tick_rate" => cfg.tick_rate = value.parse().unwrap_or(cfg.tick_rate),
        "max_clients" => cfg.max_clients = value.parse().unwrap_or(cfg.max_clients),
        "max_rooms" => cfg.max_rooms = value.parse().unwrap_or(cfg.max_rooms),
        "width" => cfg.width = value.parse().unwrap_or(cfg.width),
        "height" => cfg.height = value.parse().unwrap_or(cfg.height),
        "speed" => cfg.speed = value.parse().unwrap_or(cfg.speed),
        "abi_version" => cfg.abi_version = value.parse().unwrap_or(cfg.abi_version),
        "allow_physics" => {
            cfg.permissions.physics = value.parse().unwrap_or(cfg.permissions.physics)
        }
        "allow_lag_compensation" => {
            cfg.permissions.lag_compensation =
                value.parse().unwrap_or(cfg.permissions.lag_compensation)
        }
        "allow_logging" => {
            cfg.permissions.logging = value.parse().unwrap_or(cfg.permissions.logging)
        }
        "allow_storage" => {
            cfg.permissions.storage = value.parse().unwrap_or(cfg.permissions.storage)
        }
        "allow_messaging" => {
            cfg.permissions.messaging = value.parse().unwrap_or(cfg.permissions.messaging)
        }
        "voice_ice_servers" => cfg.voice_ice_servers = value.to_string(),
        "control_enabled" => cfg.control.enabled = value.parse().unwrap_or(cfg.control.enabled),
        "api_port" => cfg.control.api_port = value.parse().unwrap_or(cfg.control.api_port),
        "control_bind_addr" => cfg.control.bind_addr = value.to_string(),
        "control_workers" => cfg.control.workers = value.parse().unwrap_or(cfg.control.workers),
        "seat_ttl_secs" => {
            cfg.control.seat_ttl_secs = value.parse().unwrap_or(cfg.control.seat_ttl_secs)
        }
        "seat_secret" => cfg.control.seat_secret = value.to_string(),
        "rate_limit_rps" => {
            cfg.control.rate_limit_rps = value.parse().unwrap_or(cfg.control.rate_limit_rps)
        }
        "rate_limit_burst" => {
            cfg.control.rate_limit_burst = value.parse().unwrap_or(cfg.control.rate_limit_burst)
        }
        "agones_enabled" => {
            cfg.control.agones.enabled = value.parse().unwrap_or(cfg.control.agones.enabled)
        }
        "database_url" => cfg.control.session.database_url = value.to_string(),
        "signing_key" => cfg.control.session.signing_key = value.to_string(),
        "refresh_signing_key" => cfg.control.session.refresh_signing_key = value.to_string(),
        "server_key" => cfg.control.session.server_key = value.to_string(),
        "token_expires_sec" => {
            cfg.control.session.token_expiry_secs = value
                .parse()
                .unwrap_or(cfg.control.session.token_expiry_secs)
        }
        "refresh_token_expires_sec" => {
            cfg.control.session.refresh_expiry_secs = value
                .parse()
                .unwrap_or(cfg.control.session.refresh_expiry_secs)
        }
        "console_username" => cfg.control.console.username = value.to_string(),
        "console_password" => cfg.control.console.password = value.to_string(),
        "console_signing_key" => cfg.control.console.signing_key = value.to_string(),
        "console_token_expires_sec" => {
            cfg.control.console.token_expiry_secs = value
                .parse()
                .unwrap_or(cfg.control.console.token_expiry_secs)
        }
        _ => {}
    }
}
