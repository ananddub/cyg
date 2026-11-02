
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RtcInput {
    pub player_id: Arc<str>,
    pub seq: u16,
    pub tick_hint: u32,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RtcDownlink {
    pub player_id: Arc<str>,
    pub tick: u32,
    pub seq: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum UplinkEvent {

    Connected { player_id: Arc<str> },

    Disconnected { player_id: Arc<str> },

    Input(RtcInput),

    Reliable { player_id: Arc<str>, body: Vec<u8> },
}

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
