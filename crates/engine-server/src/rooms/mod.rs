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
pub use types::{MAX_OUTBOX_MESSAGES, OutboundMessage, RoomBackend, RoomLogic, RoomSnapshotData};

pub(crate) use room::{HashMap, HashSet, JournalInput, PlayerEntry, SessionSeqs, player_map, session_map};
pub(crate) use engine_manager::{CloseReason, DRAIN_REASON, lifecycle_close_code};
pub use types::close_code;

#[cfg(test)]
mod tests;
