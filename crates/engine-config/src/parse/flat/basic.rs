use crate::game::{GameConfig, Runtime};

pub(super) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
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
        "hooks" => cfg.hooks_path = value.into(),
        "port" => cfg.port = value.parse().unwrap_or(cfg.port),
        "web_port" => cfg.web_port = value.parse().unwrap_or(cfg.web_port),
        "tick_rate" => cfg.tick_rate = value.parse().unwrap_or(cfg.tick_rate),
        "patch_rate" => cfg.patch_rate = value.parse().unwrap_or(cfg.patch_rate),
        "wasm_pooling" => cfg.wasm_pooling = value.parse().unwrap_or(cfg.wasm_pooling),
        "wasm_max_memory_mb" => {
            cfg.wasm_max_memory_mb = value.parse().unwrap_or(cfg.wasm_max_memory_mb)
        }
        "tick_stagger" => cfg.tick_stagger = value.parse().unwrap_or(cfg.tick_stagger),
        "input_coalesce" => cfg.input_coalesce = value.parse().unwrap_or(cfg.input_coalesce),
        "max_clients" => cfg.max_clients = value.parse().unwrap_or(cfg.max_clients),
        "max_rooms" => cfg.max_rooms = value.parse().unwrap_or(cfg.max_rooms),
        "max_messages_per_second" => {
            cfg.max_messages_per_second = value.parse().unwrap_or(cfg.max_messages_per_second)
        }
        "room_options" => cfg.room_options = Some(value.to_string()),
        "simulate_latency_ms" => {
            cfg.simulate_latency_ms = value.parse().unwrap_or(cfg.simulate_latency_ms)
        }
        "simulate_packet_loss" | "packet_loss" | "simulate_packet_loss_percent" => {
            if let Ok(v) = value.parse::<f64>() {
                let rate = if v > 1.0 { v / 100.0 } else { v };
                cfg.simulate_packet_loss = rate.clamp(0.0, 1.0);
            }
        }
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
        _ => {}
    }
}
