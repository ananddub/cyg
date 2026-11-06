use std::collections::HashMap;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_net::framing::OpCode;
use engine_net::transport::webrtc::contract::Inbound;
use engine_net::transport::WebRtcTransport;

pub struct LoopTransport {
    pub(crate) rtc: WebRtcTransport,
    pub(crate) quic: Option<engine_net::QuicTransport>,
    pub(crate) ws: Option<engine_net::transport::WsTransport>,
    pub(crate) client_rooms: Arc<scc::HashMap<String, String>>,
    pub(crate) client_options: HashMap<String, String>,
    pub(crate) consented: std::collections::HashSet<String>,
    pub(crate) retained: HashMap<String, String>,
    pub(crate) inbound: Vec<(String, Inbound)>,
    pub(crate) latency: Duration,
    pub(crate) packet_loss_rate: f64,
    pub(crate) loss_rng: u32,
    pub(crate) dropped_packets: u64,
    pub(crate) delayed_out: VecDeque<(Instant, DelayedOut)>,
    pub(crate) delayed_in: VecDeque<(Instant, String, Inbound)>,
    pub(crate) local_addr: Option<SocketAddr>,
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
