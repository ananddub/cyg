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
pub use engine_manager::{CloseReason, DRAIN_REASON, lifecycle_close_code};

pub mod close_code {
    pub const LEFT: i32 = 1000;
    pub const KICKED: i32 = 1004;
    pub const TIMED_OUT: i32 = 1005;
    pub const LOST: i32 = 1006;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomBackend {
    Relay,
    Wasm,
    Ecs,
}

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

/// A guest's request to raise one player counter. The host validates it and
/// the control plane is the only thing that writes `player_stat`; the room
/// never names a user id or a table.
#[derive(Debug, Clone)]
pub struct StatIntent {
    pub seat: i64,
    pub stat: std::sync::Arc<str>,
    pub delta: i64,
}

/// Per-call cap, mirroring the outbox: a room that floods the queue is
/// refused instead of growing an unbounded buffer inside the tick.
pub const MAX_STAT_INTENTS: usize = 256;
