//! # Engine Net
//!
//! The network layer, split by how bytes actually reach a player. Four planes,
//! each with its own thread model:
//! - `runtime`: thread-per-core `mio` workers with `SO_REUSEPORT` distribution,
//!   microsecond epoll waits bounded by the 60Hz tick, and batched read, tick
//!   and flush passes
//! - `transport`: one folder per wire protocol behind the [`GameTransport`]
//!   trait — raw UDP datagrams and the CYG/Colyseus WebSocket protocol
//! - `transport::webrtc`: sans-IO `str0m` DataChannels, with the two game lanes
//!   and their contract in `transport::webrtc::contract`
//! - `rtc_plane`: the Tokio-resident WebRTC plane — one session mailbox per
//!   player behind a single socket reader, bridged to the game with plain data
//!
//! `framing` and `bufio` are shared by all of them: the opcode envelope that
//! goes on the wire, and the copy-free socket reads that fill `BytesMut`.

pub mod framing;
mod bufio;
pub mod rtc_plane;
pub mod runtime;
pub mod transport;
pub mod quic;

pub use bytes;
pub use framing::{OpCode, decode_frame, encode_frame, iter_frames, take_frame};
pub use rtc_plane::{
    Plane, PlaneConfig, PlaneHandle, RtcDownlink, RtcInput, RtcPlaneError, UplinkEvent,
};
pub use runtime::pool::{AggregatedTelemetry, PoolConfig, WorkerPoolHandle, spawn_worker_pool};
pub use runtime::worker::{ClientPacket, MioWorker, WorkerConfig, WorkerStats};
pub use runtime::connection::ClientConn;
pub use transport::ws::{MatchMakeError, MATCHMAKE_METHODS, close_code, response_status};
pub use transport::{
    WsTransport, GameTransport, TransportEvent,
    TransportType, UdpTransport, WebRtcServer, WebRtcServerEvent, create_transport,
};
