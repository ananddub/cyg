use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::BytesMut;
use mio::net::UdpSocket as MioUdp;
use mio::{Events, Poll};

use super::webrtc::contract::Inbound;
use crate::quic::QuicEngine;

mod events;
mod game_transport;
mod state;
#[cfg(test)]
mod tests;

pub struct QuicTransport {
    poll: Poll,
    events: Events,
    sock: MioUdp,
    engine: QuicEngine,

    by_handle: HashMap<quinn_proto::ConnectionHandle, Arc<str>>,
    by_client: HashMap<Arc<str>, quinn_proto::ConnectionHandle>,
    remote: HashMap<Arc<str>, SocketAddr>,

    reply_stream: HashMap<Arc<str>, quinn_proto::StreamId>,

    pending_reliable: HashMap<Arc<str>, Vec<Vec<u8>>>,

    reassembly: HashMap<Arc<str>, Vec<u8>>,
    inbound: Vec<(Arc<str>, Inbound)>,
    pending: Vec<super::TransportEvent>,
    next_id: u64,
}

const FRAME_HEADER: usize = 4;

fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}
