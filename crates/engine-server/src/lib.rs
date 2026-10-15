//! # engine-server — simulation core: rooms, netcode helpers, and the pinned plane
//!
//! Extracted from the root binary's `src/server.rs` (rooms + byte pipeline)
//! and `src/threads/` (mio sim plane). Owns:
//!
//! * [`rooms`] — `GameRoom`, the WASM `RoomLogic` module wrapper, host linker
//! * [`netcode`] — delta envelope wrapping, AOI delta filtering, input journaling + rollback replay
//! * [`plane`] — the pinned mio thread: raw UDP, dispatch, sim tick loop, seats

pub mod compile;
pub mod host;
pub mod netcode;
pub mod plane;
pub mod rooms;
