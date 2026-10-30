//! One browser peer: its `str0m::Rtc`, the lanes it negotiated, and the queues
//! each lane holds.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use str0m::Rtc;
use str0m::channel::ChannelId;

use super::contract::{Inbound, Lane, OutboundQueue, SeqWatermark};
use super::types::{TransportStats, WebRtcServerEvent};

/// A WebRTC peer session corresponding to one connected client.
pub struct WebRtcSession {
    pub client_id: String,
    pub rtc: Rtc,
    pub ch_unreliable: Option<ChannelId>,
    pub ch_reliable: Option<ChannelId>,
    pub channel_labels: HashMap<ChannelId, String>,
    /// Channel id to lane. Only contract channels appear here, so an inbound
    /// frame's reliability is known from the channel it arrived on instead of
    /// inferred from its bytes.
    pub lanes: HashMap<ChannelId, Lane>,
    /// One bounded drop-oldest queue per lane, indexed `lane as usize`.
    pub outbound: [OutboundQueue; 2],
    pub last_input_seq: SeqWatermark,
    pub acked_event_seq: SeqWatermark,
    pub remote_addr: Option<SocketAddr>,
    pub connected: bool,
    pub last_active: Instant,
    pub next_timeout: Option<Instant>,
    /// So a stale bundle's alias label is reported once per session rather than
    /// once per reconnect.
    pub(super) legacy_alias_warned: bool,
}

impl WebRtcSession {
    pub fn new(client_id: String, rtc: Rtc) -> Self {
        Self {
            client_id,
            rtc,
            ch_unreliable: None,
            ch_reliable: None,
            channel_labels: HashMap::new(),
            lanes: HashMap::new(),
            outbound: [OutboundQueue::default(), OutboundQueue::default()],
            last_input_seq: SeqWatermark::default(),
            acked_event_seq: SeqWatermark::default(),
            remote_addr: None,
            connected: false,
            last_active: Instant::now(),
            next_timeout: None,
            legacy_alias_warned: false,
        }
    }

    pub(super) fn channel_id(&self, lane: Lane) -> Option<ChannelId> {
        match lane {
            Lane::Unreliable => self.ch_unreliable,
            Lane::Reliable => self.ch_reliable,
        }
    }

    pub(super) fn queue(&mut self, lane: Lane) -> &mut OutboundQueue {
        &mut self.outbound[lane as usize]
    }

    pub(super) fn queue_len(&self, lane: Lane) -> usize {
        self.outbound[lane as usize].frames()
    }

    /// Throw away everything held for this session's lanes and report how many
    /// frames left: a session being replaced or removed can never deliver them.
    pub(super) fn discard_outbound(&mut self) -> usize {
        self.outbound.iter_mut().map(|q| q.clear()).sum()
    }
}

/// Everything a session's `str0m` output can produce. One field so it can be
/// borrowed alongside `sessions` — field-level split borrows do not extend
/// through a method call on `&mut self`.
#[derive(Default)]
pub(super) struct Sink {
    pub(super) events: Vec<WebRtcServerEvent>,
    pub(super) inbound: Vec<(String, Inbound)>,
    pub(super) pending: Vec<(SocketAddr, Vec<u8>)>,
    pub(super) stats: TransportStats,
}
