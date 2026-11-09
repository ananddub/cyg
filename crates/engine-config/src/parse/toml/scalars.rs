use crate::game::{GameConfig, Runtime};
use crate::transports::Lane;

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
    // Retired: lanes are switched in `[transports]` now. A legacy `quic_port`
    // still means what it always meant — "quic on at this port" — so it turns the
    // lane on and is otherwise ignored, since ports are assigned by the OS.
    if let Some(v) = int("quic_port") {
        tracing::warn!(
            "[config] `quic_port` is retired; use [transports] quic = true \
             (port is assigned automatically)"
        );
        if v > 0 {
            cfg.transports.set_enabled(Lane::Quic, true);
        }
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
    if let Some(v) = int("tick_stagger") {
        cfg.tick_stagger = (v.max(1).min(64)) as u32;
    }
    if let Some(v) = boolean("input_coalesce") {
        cfg.input_coalesce = v;
    }
    if let Some(v) = boolean("wasm_pooling") {
        cfg.wasm_pooling = v;
    }
    if let Some(v) = int("wasm_max_memory_mb") {
        cfg.wasm_max_memory_mb = v.max(1) as u32;
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
    // Retired alongside `quic_port`: `ws_port = 0` used to mean "WebSocket off".
    if let Some(v) = int("ws_port") {
        tracing::warn!(
            "[config] `ws_port` is retired; use [transports] websocket = true/false \
             (port is assigned automatically)"
        );
        cfg.transports.set_enabled(Lane::WebSocket, v > 0);
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
