//! The plane's public models: what crosses the bridge in either direction.
//!
//! These are deliberately plain data — no str0m or Tokio type leaks across them
//! — so a host can carry them over a crossbeam bus.

use std::sync::Arc;

/// Inbound (uplink) one client input frame emitted by the plane.
///
/// `body` is the **raw** DataChannel payload; the plane intentionally does not
/// decode the game-specific input header. `seq`/`tick_hint` are best-effort
/// placeholders (zero) that the bridge overwrites when it decodes `body`. They
/// exist here so the plane→host hop needs no second allocation.
#[derive(Debug, Clone)]
pub struct RtcInput {
    pub player_id: Arc<str>,
    pub seq: u16,
    pub tick_hint: u32,
    pub body: Vec<u8>,
}

/// Outbound (downlink) one server delta handed to the plane.
///
/// `body` is an un-fragmented delta payload; the plane fragments it with the
/// codec (`MAX_DELTA_CHUNK` per frame) and writes the frames to the unreliable
/// channel. A `body` larger than `MAX_DELTA_BODY` is rejected, never truncated.
/// `seq` is room-wide (same for every recipient of a tick) and `tick` mirrors
/// `room.current_tick`.
#[derive(Debug, Clone)]
pub struct RtcDownlink {
    pub player_id: Arc<str>,
    pub tick: u32,
    pub seq: u16,
    pub body: Vec<u8>,
}

/// A message leaving the plane toward the game loop / bridge.
#[derive(Debug, Clone)]
pub enum UplinkEvent {
    /// Client connected and both channels are established.
    Connected { player_id: Arc<str> },
    /// Client disconnected or the session was disposed.
    Disconnected { player_id: Arc<str> },
    /// Raw unreliable-channel input from a client.
    Input(RtcInput),
    /// Reliable-channel payload from a client (events / acks).
    Reliable { player_id: Arc<str>, body: Vec<u8> },
}

/// Errors returned across the plane's public surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtcPlaneError {
    Signaling(String),
    Transport(String),
    ChannelNotOpen(String),
    DeltaTooLarge(usize),
    NoSuchSession,
    PlayerUnknown,
    PlaneGone,
}

impl std::fmt::Display for RtcPlaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RtcPlaneError::Signaling(s) => write!(f, "signaling error: {s}"),
            RtcPlaneError::Transport(s) => write!(f, "transport error: {s}"),
            RtcPlaneError::ChannelNotOpen(label) => write!(f, "channel '{label}' not open"),
            RtcPlaneError::DeltaTooLarge(n) => write!(f, "delta body of {n} bytes is oversized"),
            RtcPlaneError::NoSuchSession => write!(f, "no such session"),
            RtcPlaneError::PlayerUnknown => write!(f, "unknown player id"),
            RtcPlaneError::PlaneGone => write!(f, "plane is shut down"),
        }
    }
}

impl std::error::Error for RtcPlaneError {}
