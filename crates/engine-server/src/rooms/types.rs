use engine_wasm::wasm_module;
use serde::{Deserialize, Serialize};

wasm_module! {
    pub struct RoomLogic {
        fn on_create(max: i32);
        fn on_join(id: i64, x: f64, y: f64);
        fn on_leave(id: i64);
        fn on_alloc(len: i32) -> i32;
        fn on_input(id: i64, ptr: i32, len: i32);
        fn on_tick(dt: f64) -> i64;
        fn on_save() -> i64;
        fn on_load(ptr: i32, len: i32);
    }
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
    pub event: String,
    pub payload: Vec<u8>,
}

pub const MAX_OUTBOX_MESSAGES: usize = 256;
