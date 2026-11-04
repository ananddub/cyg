use crate::game::{GameConfig, Runtime};

pub(super) fn apply(cfg: &mut GameConfig, table: &toml::value::Table) {
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
    if let Some(v) = int("patch_rate") {
        cfg.patch_rate = (v.max(0)) as u32;
    }
    if let Some(v) = int("max_clients") {
        cfg.max_clients = v as i32;
    }
    if let Some(v) = int("max_rooms") {
        cfg.max_rooms = v as u32;
    }
    if let Some(v) = int("max_messages_per_second") {
        cfg.max_messages_per_second = v.max(0) as u32;
    }
    if let Some(v) = string("room_options") {
        cfg.room_options = Some(v);
    }
    if let Some(v) = int("ws_port") {
        cfg.ws_port = v.max(0) as u16;
    }
    if let Some(v) = int("simulate_latency_ms") {
        cfg.simulate_latency_ms = v.max(0) as u32;
    }
    if let Some(v) = float("simulate_packet_loss")
        .or_else(|| float("packet_loss"))
        .or_else(|| float("simulate_packet_loss_percent"))
    {
        let rate = if v > 1.0 { v / 100.0 } else { v };
        cfg.simulate_packet_loss = rate.clamp(0.0, 1.0);
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
}
