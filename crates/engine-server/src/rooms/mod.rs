mod game_room;
mod host_data;
mod host_data_2d;
mod host_data_3d;
mod host_data_timers;
mod linker;
mod room;
mod server;
mod server_rooms;
mod types;

pub use host_data::{RoomHostData, TimerState};
pub use linker::build_host_linker;
pub use room::GameRoom;
pub use server::GameServer;
pub use types::{
    MAX_OUTBOX_MESSAGES, OutboundMessage, RoomBackend, RoomLogic, RoomSnapshotData,
};

pub(crate) use room::JournalInput;
pub(crate) use types::{close_code, CloseReason, DRAIN_REASON};

#[cfg(test)]
mod tests;
