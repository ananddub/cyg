//! `[transports]` — the four lane switches.
//!
//! Only switches are read. Ports live nowhere in this table on purpose: the game
//! port is `port`, and a lane the operator turns on takes whatever ephemeral
//! port the OS gives it and reports that port at boot.

use crate::game::GameConfig;
use crate::transports::Lane;

pub(super) fn apply(cfg: &mut GameConfig, table: &toml::value::Table) {
    let Some(block) = table.get("transports").and_then(|v| v.as_table()) else {
        return;
    };
    for lane in Lane::ALL {
        if let Some(enabled) = block.get(lane.as_str()).and_then(|v| v.as_bool()) {
            cfg.transports.set_enabled(lane, enabled);
        }
    }
    // Unknown keys are refused rather than ignored: a typo such as
    // `webscoket = false` would otherwise leave the lane silently on.
    let mut unknown: Vec<&str> = block
        .keys()
        .map(String::as_str)
        .filter(|key| Lane::parse(key).is_none())
        .collect();
    if !unknown.is_empty() {
        unknown.sort_unstable();
        tracing::warn!(
            "[config] [transports] has unknown keys: {} (known: webrtc, websocket, udp, quic)",
            unknown.join(", ")
        );
    }
}
