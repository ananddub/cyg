
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use str0m::Rtc;
use str0m::channel::ChannelId;

use super::contract::{Inbound, Lane, OutboundQueue, SeqWatermark};
use super::types::{TransportStats, WebRtcServerEvent};

pub struct WebRtcSession {
    pub client_id: String,
    pub rtc: Rtc,
    pub ch_unreliable: Option<ChannelId>,
    pub ch_reliable: Option<ChannelId>,
    pub channel_labels: HashMap<ChannelId, String>,

    pub lanes: HashMap<ChannelId, Lane>,

    pub outbound: [OutboundQueue; 2],
    pub last_input_seq: SeqWatermark,
    pub acked_event_seq: SeqWatermark,
    pub remote_addr: Option<SocketAddr>,
    pub connected: bool,
    pub last_active: Instant,
    pub next_timeout: Option<Instant>,

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

    pub(super) fn discard_outbound(&mut self) -> usize {
        self.outbound.iter_mut().map(|q| q.clear()).sum()
    }
}

#[derive(Default)]
pub(super) struct Sink {
    pub(super) events: Vec<WebRtcServerEvent>,
    pub(super) inbound: Vec<(String, Inbound)>,
    pub(super) pending: Vec<(SocketAddr, Vec<u8>)>,
    pub(super) stats: TransportStats,
}
