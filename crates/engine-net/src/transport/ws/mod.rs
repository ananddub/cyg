//! # OpenOxide WebSocket Wire-Protocol Transport
//!
//! Binary game protocol over WebSocket — the browser fallback plane:
//! - Standard RFC 6455 WebSocket Upgrade handshake (`101 Switching Protocols`)
//! - RFC 6455 binary frame decoding with client unmasking, fragmentation,
//!   control frames (`ping`/`pong`/`close`) and server heartbeat
//! - Matchmake seat-reservation endpoint (`POST /matchmake/<method>/<room>`)
//!   with the full method whitelist (`joinOrCreate`, `create`, `join`,
//!   `joinById`, `reconnect`) and typed `MatchMakeError` rejections
//! - wire protocol opcodes (packages/shared-types/src/Protocol.ts), with
//!   modifier bits (`PROTOCOL_CODE_MASK`) stripped before dispatch:
//!   - `10` -> `JOIN_ROOM` handshake (server-initiated right after upgrade)
//!   - `11` -> `ERROR` (server-initiated, surfaces to `room.onError`)
//!   - `12` -> `LEAVE_ROOM` (client leave; server answers close `4000 CONSENTED`)
//!   - `13` -> `ROOM_DATA` (client messages -> game layer)
//!   - `14` -> `ROOM_STATE` (full snapshots via `OpCode::StateSnapshot`)
//!   - `15` -> `ROOM_STATE_PATCH` (server state deltas via `OpCode::StateDelta`)
//!   - `17` -> `ROOM_DATA_BYTES` (binary messages via `OpCode::RoomDataBytes`)
//!   - `18` -> `PING` (echoed back — powers `room.ping()` and `client.getLatency()`)
//!   - `21` -> `ROOM_REQUEST` (client request/response RPC via `OpCode::RoomRequest`)
//!   - `22` -> `ROOM_RESPONSE` (server reply via `OpCode::RoomResponse`)
//! - Reconnection: disconnecting sessions keep their reconnection token valid
//!   (`retain_session`) so `matchmake/reconnect` can re-issue the seat
//!
//! ## Layout
//!
//! `eventloop` holds the single ordered pass (`impl GameTransport`) and the
//! send surface; `accept` and `client_io` are its two event arms and must stay
//! one pass. `http` parses and answers the greeting, `frames` decodes the game
//! protocol, `rfc6455` is the framing and handshake crypto, and `protocol`,
//! `client`, `hooks`, `heartbeat`, `msgpack`, `error` and `transport` hold the
//! constants, session model, borrow bundles, heartbeat, msgpack writers,
//! rejections and the transport struct itself.

mod accept;
mod client;
mod client_io;
mod error;
mod eventloop;
mod frames;
mod heartbeat;
mod hooks;
mod http;
mod msgpack;
mod protocol;
mod publish;
mod rfc6455;
mod transport;

pub use error::MatchMakeError;
pub use heartbeat::HeartbeatConfig;
pub use hooks::HttpHook;
pub use protocol::{MATCHMAKE_METHODS, close_code, response_status};
pub use transport::WsTransport;
