use std::collections::HashMap;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_net::framing::OpCode;
use engine_net::transport::WebRtcTransport;
use engine_net::transport::webrtc::contract::Inbound;

pub struct LoopTransport {
    pub(crate) rtc: WebRtcTransport,
    pub(crate) quic: Option<engine_net::QuicTransport>,
    pub(crate) ws: Option<engine_net::transport::WsTransport>,
    /// Seat bookkeeping for non-WS lanes (WebRTC/UDP). One entry per session
    /// instead of the two parallel maps this used to keep.
    pub(crate) seats: HashMap<String, SeatEntry>,
    pub(crate) consented: std::collections::HashSet<String>,
    pub(crate) retained: HashMap<String, String>,
    pub(crate) inbound: Vec<(Arc<str>, Inbound)>,
    pub(crate) latency: Duration,
    pub(crate) packet_loss_rate: f64,
    pub(crate) loss_rng: u32,
    pub(crate) dropped_packets: u64,
    pub(crate) delayed_out: VecDeque<(Instant, DelayedOut)>,
    /// Separate sequence space for the must-deliver state stream when a schema
    /// opts into `.unreliable()` fields.
    pub(crate) reliable_seqs: HashMap<String, u16>,
    pub(crate) delayed_in: VecDeque<(Instant, Arc<str>, Inbound)>,
    pub(crate) local_addr: Option<SocketAddr>,
}

/// Room + options recorded when a seat is registered on a non-WS lane.
#[derive(Debug, Clone, Default)]
pub(crate) struct SeatEntry {
    pub(crate) room: Arc<str>,
    pub(crate) options: Arc<str>,
}

#[derive(Debug)]
pub(crate) enum DelayedOut {
    Op {
        client_id: String,
        opcode: OpCode,
        payload: Vec<u8>,
    },
    State {
        client_id: String,
        payload: Vec<u8>,
    },
    Delta {
        session: String,
        seq: u16,
        tick: u32,
        body: Vec<u8>,
    },
}
