use crate::game::{GameConfig, Runtime};

pub(crate) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
    match key {
        "name" => cfg.name = value.to_string(),
        "game_id" => cfg.game_id = Some(value.to_string()),
        "game_key" => cfg.game_key = Some(value.to_string()),
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
        "quic_port" => cfg.quic_port = value.parse().unwrap_or(cfg.quic_port),
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
        "database_endpoint" | "database_url" => {
            cfg.control.session.database_endpoint = value.to_string()
        }
        "database_ns" => cfg.control.session.database_ns = value.to_string(),
        "database_db" => cfg.control.session.database_db = value.to_string(),
        "database_user" => cfg.control.session.database_user = value.to_string(),
        "database_pass" => cfg.control.session.database_pass = value.to_string(),
        "database_memtable_mib" => {
            cfg.control.session.database_memtable_mib = value
                .parse()
                .unwrap_or(cfg.control.session.database_memtable_mib)
        }
        "database_cache_mib" => {
            cfg.control.session.database_cache_mib = value
                .parse()
                .unwrap_or(cfg.control.session.database_cache_mib)
        }
        "blobs" => cfg.control.session.blobs = value.to_string(),
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
        "iap_apple_shared_secret" => cfg.control.iap.apple_shared_secret = value.to_string(),
        "iap_apple_verify_url" => cfg.control.iap.apple_verify_url = value.to_string(),
        "iap_apple_sandbox_url" => cfg.control.iap.apple_sandbox_url = value.to_string(),
        "iap_google_service_account" => {
            cfg.control.iap.google_service_account = value.to_string()
        }
        "iap_google_package_name" => cfg.control.iap.google_package_name = value.to_string(),
        "iap_google_token_url" => cfg.control.iap.google_token_url = value.to_string(),
        "iap_google_api_base" => cfg.control.iap.google_api_base = value.to_string(),
        "iap_allow_unverified" => {
            cfg.control.iap.allow_unverified = value.parse().unwrap_or(cfg.control.iap.allow_unverified)
        }
        "social_google_client_ids" => {
            cfg.control.social.google_client_ids = crate::social::split_ids(value)
        }
        "social_google_tokeninfo_url" => {
            cfg.control.social.google_tokeninfo_url = value.to_string()
        }
        "social_apple_client_id" => cfg.control.social.apple_client_id = value.to_string(),
        "social_apple_keys_url" => cfg.control.social.apple_keys_url = value.to_string(),
        "social_facebook_app_id" => cfg.control.social.facebook_app_id = value.to_string(),
        "social_facebook_app_secret" => cfg.control.social.facebook_app_secret = value.to_string(),
        "social_facebook_graph_url" => cfg.control.social.facebook_graph_url = value.to_string(),
        "social_steam_app_id" => {
            cfg.control.social.steam_app_id =
                value.parse().unwrap_or(cfg.control.social.steam_app_id)
        }
        "social_steam_web_api_key" => cfg.control.social.steam_web_api_key = value.to_string(),
        "social_steam_api_url" => cfg.control.social.steam_api_url = value.to_string(),
        "social_allow_unverified" => {
            cfg.control.social.allow_unverified = value
                .parse()
                .unwrap_or(cfg.control.social.allow_unverified)
        }
        _ => {}
    }
}
