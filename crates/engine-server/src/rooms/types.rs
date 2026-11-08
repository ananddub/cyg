use engine_wasm::wasm_module;
use serde::{Deserialize, Serialize};

wasm_module! {
    pub struct RoomLogic {
        // on_create / on_join / on_auth are NOT declared here on purpose:
        // their signatures evolved (ABI v2) and older uploaded modules still
        // export the previous shapes. GameRoom's call helpers probe the
        // instance for the newest signature and fall back to the old one.
        /// `on_leave(id, code)` — a CloseCode: 1000 client left, 1001 server
        /// shutdown, 1005 reconnect window expired, 1006 lost connection.
        fn on_leave(id: i64, code: i32);
        fn on_alloc(len: i32) -> i32;
        fn on_input(id: i64, ptr: i32, len: i32);
        fn on_tick(dt: f64) -> i64;
        fn on_save() -> i64;
        fn on_load(ptr: i32, len: i32);
    }
}

/// Room leave codes the host hands to `on_leave(id, code)` — the guest
/// mirror lives in the SDK as `CloseCode`.
pub mod close_code {
    pub const LEFT: i32 = 1000;
    pub const KICKED: i32 = 1004;
    pub const SERVER_SHUTDOWN: i32 = 1001;
    pub const TIMED_OUT: i32 = 1005;
    pub const LOST: i32 = 1006;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomBackend {
    Relay,
    Wasm,
    Ecs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseReason {
    Empty,
    Duration,
}

impl CloseReason {
    pub(crate) fn label(self) -> &'static str {
        match self {
            CloseReason::Empty => "empty",
            CloseReason::Duration => "duration",
        }
    }
}

pub(crate) const DRAIN_REASON: &str = "drain";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RoomSnapshotData {
    Wasm(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundMessage {
    pub target: i64,
    pub event: std::sync::Arc<str>,
    pub payload: bytes::Bytes,
}

pub const MAX_OUTBOX_MESSAGES: usize = 256;
