//! Which network lanes this server opens.
//!
//! A lane used to be implied by its port: `quic_port = 0` meant "off" and any
//! other value meant "on", while WebRTC and the native UDP socket were always
//! opened and could not be closed at all. That left the operator configuring
//! ports they never chose, and produced collisions between lanes that were
//! enabled without anyone asking for them.
//!
//! So this asks the only question the operator actually has an opinion about —
//! which lanes run — and leaves addressing to the server: one port for the
//! game, and ephemeral ports for the extra lanes, which the server reports in
//! its startup log and in the `/status` document clients can read.

use std::collections::BTreeMap;

/// The lanes a server can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lane {
    /// WebRTC DataChannel — browsers and any client with an ICE stack.
    WebRtc,
    /// WebSocket room-data lane — Godot / Unity / Unreal / cyg.js.
    WebSocket,
    /// Raw UDP — the native-engine plugin path.
    Udp,
    /// QUIC / WebTransport — same wire contract as WebRTC.
    Quic,
}

impl Lane {
    pub const ALL: [Lane; 4] = [Lane::WebRtc, Lane::WebSocket, Lane::Udp, Lane::Quic];

    /// The name this lane answers to in the config and in logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Lane::WebRtc => "webrtc",
            Lane::WebSocket => "websocket",
            Lane::Udp => "udp",
            Lane::Quic => "quic",
        }
    }

    /// What a client uses this lane for, printed once at boot so an operator
    /// reading the log can tell which lane serves which engine.
    pub fn purpose(self) -> &'static str {
        match self {
            Lane::WebRtc => "browsers, low-latency DataChannel",
            Lane::WebSocket => "Godot / Unity / Unreal / cyg.js",
            Lane::Udp => "native engine plugins",
            Lane::Quic => "QUIC / WebTransport clients",
        }
    }

    pub fn parse(name: &str) -> Option<Lane> {
        match name.trim().to_ascii_lowercase().as_str() {
            "webrtc" | "webrtc_datachannel" => Some(Lane::WebRtc),
            "websocket" | "ws" => Some(Lane::WebSocket),
            "udp" | "native_udp" => Some(Lane::Udp),
            "quic" | "webtransport" => Some(Lane::Quic),
            _ => None,
        }
    }
}

/// One lane's switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneConfig {
    pub enabled: bool,
}

/// The `[transports]` table: four booleans, no ports.
#[derive(Debug, Clone)]
pub struct TransportsConfig {
    lanes: BTreeMap<Lane, LaneConfig>,
}

impl Default for TransportsConfig {
    /// WebRTC and WebSocket are on because they need nothing extra from the
    /// operator: a browser and every supported engine can reach them today. UDP
    /// and QUIC are off because they previously collided with a port nobody
    /// configured, and turning a listener on should be a decision.
    fn default() -> Self {
        let mut lanes = BTreeMap::new();
        lanes.insert(Lane::WebRtc, LaneConfig { enabled: true });
        lanes.insert(Lane::WebSocket, LaneConfig { enabled: true });
        lanes.insert(Lane::Udp, LaneConfig { enabled: false });
        lanes.insert(Lane::Quic, LaneConfig { enabled: false });
        Self { lanes }
    }
}

impl TransportsConfig {
    pub fn get(&self, lane: Lane) -> &LaneConfig {
        // Every lane is inserted by `default()`; the fallback only keeps the
        // signature infallible.
        self.lanes
            .get(&lane)
            .unwrap_or_else(|| panic!("lane {} missing from config", lane.as_str()))
    }

    pub fn enabled(&self, lane: Lane) -> bool {
        self.get(lane).enabled
    }

    pub fn set_enabled(&mut self, lane: Lane, enabled: bool) {
        if let Some(cfg) = self.lanes.get_mut(&lane) {
            cfg.enabled = enabled;
        }
    }

    /// The lanes this config asks to open, in a stable order.
    pub fn active(&self) -> Vec<Lane> {
        Lane::ALL
            .into_iter()
            .filter(|lane| self.enabled(*lane))
            .collect()
    }

    /// The lanes this config leaves closed, in the same order.
    pub fn inactive(&self) -> Vec<Lane> {
        Lane::ALL
            .into_iter()
            .filter(|lane| !self.enabled(*lane))
            .collect()
    }

    /// Reject a config with no network lane at all.
    ///
    /// Ports are not validated here because there are none to validate: the game
    /// port is `port`, and every other lane takes what the OS hands it.
    pub fn validate(&self) -> Result<(), String> {
        if self.active().is_empty() {
            return Err(format!(
                "no transports enabled — turn on at least one of {} under [transports]",
                Lane::ALL
                    .iter()
                    .map(|lane| lane.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Ok(())
    }

    /// The one-line summary printed at boot, e.g.
    /// `transports: webrtc, websocket (udp, quic off)`.
    pub fn summary(&self) -> String {
        let on: Vec<&str> = self.active().iter().map(|lane| lane.as_str()).collect();
        let off: Vec<&str> = self.inactive().iter().map(|lane| lane.as_str()).collect();
        if off.is_empty() {
            format!("transports: {}", on.join(", "))
        } else {
            format!("transports: {} ({} off)", on.join(", "), off.join(", "))
        }
    }
}
