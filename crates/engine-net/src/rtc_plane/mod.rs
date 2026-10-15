//! # Tokio-resident WebRTC data plane
//!
//! This is the WebRTC data plane that runs **inside Tokio**, entirely separate
//! from the `mio` simulation thread. It owns:
//!
//! - a single bound UDP socket,
//! - one Tokio runtime driven by **one dedicated OS thread**,
//! - a demultiplexer task that routes datagrams by remote [`std::net::SocketAddr`]
//!   to per-session tasks through bounded channels,
//! - one task per browser client, each owning its own [`RtcSession`] (`str0m::Rtc`).
//!
//! **No session shares a mutex with another.** Per-session state lives only in
//! its own task; the demux task owns the address registry (no lock — it is the
//! sole reader of the socket), and the only shared structures are *registries of
//! senders* (`outbox`, `player_index`) that the hot downlink path reads to find a
//! session's inbox. Nothing here touches crypto or sockets on the mio thread:
//! DTLS/SCTP/scheduling all happen on the plane thread.
//!
//! ## Driving a plane
//!
//! A caller holds a [`PlaneHandle`] (cloneable, `Send + Sync`) and never the plane
//! itself: `create_session` returns the SDP offer for the browser, `apply_answer`
//! and `add_candidate` relay the browser's SDP and trickle ICE,
//! [`PlaneHandle::send_delta`] pushes world deltas down, and
//! [`PlaneHandle::recv_uplink`] drains client input back up. Signaling and the game
//! loop run on different threads, which is why both directions are channels rather
//! than callbacks.

mod config;
mod demux;
mod handle;
mod inbox;
mod plane;
mod registries;
mod session;
mod task;
mod types;

pub use config::PlaneConfig;
pub use handle::PlaneHandle;
pub use plane::Plane;
pub use session::{
    LABEL_RELIABLE, LABEL_UNRELIABLE, Outbound, PlaneChannel, RtcSession, SessionEvent,
};
pub use types::{RtcDownlink, RtcInput, RtcPlaneError, UplinkEvent};
