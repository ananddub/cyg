//! # Per-client WebRTC session (sans-io pump over `str0m::Rtc`)
//!
//! One [`RtcSession`] owns exactly one `str0m::Rtc` for one browser peer and
//! implements the canonical str0m run loop the async task in [`crate::rtc_plane`]
//! drives:
//!
//! ```text
//! poll_timeout -> sleep_until -> handle_input(timeout | datagram) -> drain poll_output
//! ```
//!
//! str0m 0.24 has no `Net<Server>` type; the sans-io engine *is* `Rtc`, so this
//! module wraps `Rtc` and presents the `Outbound` surface the plane needs:
//! [`Outbound::Datagram`] (a UDP write) and [`Outbound::Data`] (received
//! DataChannel payload). Every call that mutates `Rtc` here immediately drains
//! `poll_output` to the next [`str0m::Output::Timeout`], upholding str0m's
//! single-mutation invariant (mutate → drain → mutate).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_webrtc_protocol::channel::{
    MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame,
};
use str0m::change::{SdpAnswer, SdpPendingOffer};
use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

use crate::rtc_plane::RtcPlaneError;

/// Label contract: SCTP-unordered, 0-retransmits high-rate input / delta channel.
pub const LABEL_UNRELIABLE: &str = "game-unreliable";

/// Label contract: ordered, reliable channel for critical events.
pub const LABEL_RELIABLE: &str = "game-reliable";

/// Wire MTU budget str0m sizes SCTP/DTLS records to. The codec already keeps each
/// DataChannel message under `MAX_FRAME` (1200); this target leaves headroom for
/// SCTP + DTLS framing so no *datagram* exceeds the 1200-byte path budget.
const WIRE_MTU_TARGET: usize = 1100;

/// The WebRTC channel a payload arrived on / should be sent on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneChannel {
    Unreliable,
    Reliable,
}

/// One step of output produced by draining the engine.
#[derive(Debug)]
pub enum Outbound {
    /// Put these bytes on the UDP socket toward `destination`.
    Datagram { destination: SocketAddr, contents: Vec<u8> },
    /// A DataChannel payload received from the remote peer.
    Data { channel: ChannelId, binary: bool, payload: Vec<u8> },
    /// A lifecycle transition the plane must react to.
    Event(SessionEvent),
}

/// Lifecycle events surfaced from the str0m engine to the session task.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Connected,
    Closed,
    ChannelOpen { id: ChannelId, label: String },
    ChannelClose { id: ChannelId },
    /// The session accepted its first packet from `addr`; the demux can now pin
    /// `addr -> session` and stop broadcasting unknown datagrams to it.
    RemoteAddrBound { addr: SocketAddr },
}

/// A single browser peer's WebRTC session, driven by its own Tokio task.
pub struct RtcSession {
    rtc: Rtc,
    /// The offer we produced, awaiting the remote answer to complete negotiation.
    pending: Option<SdpPendingOffer>,
    /// Destination address we stamp on inbound [`Receive`]s. str0m only accepts a
    /// datagram whose destination matches one of our local candidates, so for a
    /// socket bound to `0.0.0.0` the demux passes the concrete announced address.
    local_addr: SocketAddr,

    session_id: Arc<str>,

    ch_unreliable: Option<ChannelId>,
    ch_reliable: Option<ChannelId>,
    channel_labels: HashMap<ChannelId, String>,

    remote_addr: Option<SocketAddr>,
    connected: bool,
    next_timeout: Instant,
}

/// Unreliable channel config: unordered, drop-after-1-retransmit budget.
///
/// The browser `RTCDataChannelInit` cannot request both `maxRetransmits: 0` and a
/// `maxPacketLifeTime` at once (they are mutually exclusive per the WebRTC spec),
/// so we honor the codec's documented `{ ordered: false, maxRetransmits: 0 }`
/// contract — pure fire-and-forget. A stale tick is superseded by the next tick
/// within ~20 ms, which is tighter than any packet lifetime we would pick.
fn unreliable_config() -> ChannelConfig {
    ChannelConfig {
        label: LABEL_UNRELIABLE.to_string(),
        ordered: false,
        reliability: Reliability::MaxRetransmits { retransmits: 0 },
        ..Default::default()
    }
}

/// Reliable channel config: ordered with unlimited retransmits (`Reliable`).
fn reliable_config() -> ChannelConfig {
    ChannelConfig {
        label: LABEL_RELIABLE.to_string(),
        ordered: true,
        reliability: Reliability::Reliable,
        ..Default::default()
    }
}

impl RtcSession {
    /// Create a server session that *offers* both game DataChannels and returns
    /// the SDP offer string to hand to the remote browser via signaling.
    ///
    /// `candidates` are the addresses the server advertises as host ICE
    /// candidates; `local_addr` is the concrete destination address inbound
    /// datagrams will carry (used to build [`Receive`]).
    pub fn new_server(
        session_id: Arc<str>,
        local_addr: SocketAddr,
        candidates: &[SocketAddr],
    ) -> Result<(Self, String), RtcPlaneError> {
        let now = Instant::now();
        let mut rtc = Rtc::builder()
            .set_mtu(WIRE_MTU_TARGET..=WIRE_MTU_TARGET + 100)
            .build(now);

        for addr in candidates {
            let cand = Candidate::host(*addr, Protocol::Udp)
                .map_err(|e| RtcPlaneError::Signaling(format!("bad candidate {addr}: {e:?}")))?;
            rtc.add_local_candidate(cand);
        }

        let mut api = rtc.sdp_api();
        let _ch_unreliable = api.add_channel_with_config(unreliable_config());
        let _ch_reliable = api.add_channel_with_config(reliable_config());
        let (offer, pending) = api
            .apply()
            .ok_or_else(|| RtcPlaneError::Signaling("no negotiation produced".into()))?;
        let offer_sdp = offer.to_sdp_string();

        let mut session = RtcSession {
            rtc,
            pending: Some(pending),
            local_addr,
            session_id,
            ch_unreliable: Some(_ch_unreliable),
            ch_reliable: Some(_ch_reliable),
            channel_labels: HashMap::new(),
            remote_addr: None,
            connected: false,
            next_timeout: now + Duration::from_secs(3600),
        };
        // Prime the first timeout; DTLS/ICE scheduling only starts after a drain.
        let _ = session.drain();
        Ok((session, offer_sdp))
    }

    /// Apply the remote SDP answer to the offer we produced.
    pub fn accept_answer(&mut self, sdp: &str) -> Result<Vec<Outbound>, RtcPlaneError> {
        let answer = SdpAnswer::from_sdp_string(sdp)
            .map_err(|e| RtcPlaneError::Signaling(format!("invalid answer: {e:?}")))?;
        let pending = self
            .pending
            .take()
            .ok_or(RtcPlaneError::Signaling("no pending offer to answer".into()))?;
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|e| RtcPlaneError::Signaling(format!("accept_answer failed: {e:?}")))?;
        Ok(self.drain())
    }

    /// Feed a remote ICE candidate (SDP `candidate:` line) to the engine.
    pub fn add_remote_candidate(&mut self, sdp: &str) -> Result<Vec<Outbound>, RtcPlaneError> {
        let trimmed = sdp.trim().trim_start_matches("a=").trim();
        let candidate = Candidate::from_sdp_string(trimmed)
            .or_else(|_| {
                let stripped = trimmed.trim_start_matches("candidate:").trim();
                Candidate::from_sdp_string(stripped)
            })
            .map_err(|e| RtcPlaneError::Signaling(format!("bad candidate: {e:?}")))?;
        self.rtc.add_remote_candidate(candidate);
        Ok(self.drain())
    }

    /// Does this datagram belong to us? Cheap ICE/STUN affinity check.
    pub fn accepts(&self, now: Instant, from: SocketAddr, payload: &[u8]) -> bool {
        match Receive::new(Protocol::Udp, from, self.local_addr, payload) {
            Ok(recv) => self.rtc.accepts(&Input::Receive(now, recv)),
            Err(_) => false,
        }
    }

    /// Ingest one inbound UDP datagram. Call only when [`Self::accepts`] agreed,
    /// or once the peer address is already bound.
    pub fn feed_datagram(
        &mut self,
        now: Instant,
        from: SocketAddr,
        payload: &[u8],
    ) -> Result<Vec<Outbound>, RtcPlaneError> {
        let recv = Receive::new(Protocol::Udp, from, self.local_addr, payload)
            .map_err(|e| RtcPlaneError::Transport(format!("receive build: {e:?}")))?;
        let first_binding = self.remote_addr.is_none();
        self.rtc
            .handle_input(Input::Receive(now, recv))
            .map_err(|e| RtcPlaneError::Transport(format!("handle_input: {e:?}")))?;
        if self.remote_addr.is_none() {
            self.remote_addr = Some(from);
        }
        let mut out = self.drain();
        if first_binding && self.remote_addr.is_some() {
            out.push(Outbound::Event(SessionEvent::RemoteAddrBound { addr: from }));
        }
        Ok(out)
    }

    /// Feed a timeout. Call when `now >= poll_timeout()` and no datagram arrived.
    pub fn feed_timeout(&mut self, now: Instant) -> Vec<Outbound> {
        let _ = self.rtc.handle_input(Input::Timeout(now));
        self.drain()
    }

    /// The instant this session next wants a timeout input.
    pub fn poll_timeout(&self) -> Instant {
        self.next_timeout
    }

    /// Convert a str0m `Instant` timeout into a tokio-friendly `Duration` from now,
    /// saturating to zero if it already elapsed (the caller then pumps immediately).
    pub fn timeout_delay(&self) -> Duration {
        self.next_timeout.saturating_duration_since(Instant::now())
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn session_id(&self) -> &Arc<str> {
        &self.session_id
    }

    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    /// Which channel id is which, once opened.
    pub fn channel_ids(&self) -> (Option<ChannelId>, Option<ChannelId>) {
        (self.ch_unreliable, self.ch_reliable)
    }

    /// Fragment a delta body with the codec and write every frame to the
    /// unreliable channel. Never hands str0m a blob larger than `MAX_FRAME`.
    ///
    /// Returns the datagrams the engine produced as a result (may be empty until
    /// the channel is open). An oversized body (beyond `MAX_DELTA_BODY`) is
    /// rejected rather than silently truncated.
    pub fn send_delta(
        &mut self,
        seq: u16,
        tick: u32,
        body: &[u8],
    ) -> Result<Vec<Outbound>, RtcPlaneError> {
        let cid = self
            .ch_unreliable
            .ok_or(RtcPlaneError::ChannelNotOpen(LABEL_UNRELIABLE.into()))?;
        let total = delta_fragment_count(body.len())
            .ok_or(RtcPlaneError::DeltaTooLarge(body.len()))?;

        let mut out = Vec::new();
        let mut frame = Vec::new();
        for idx in 0..total {
            let start = idx as usize * MAX_DELTA_CHUNK;
            let end = (start + MAX_DELTA_CHUNK).min(body.len());
            write_delta_frame(&mut frame, seq, tick, idx, total, &body[start..end]);
            if let Some(mut ch) = self.rtc.channel(cid) {
                // `true` = binary payload. A full unreliable send buffer makes
                // str0m drop the newest frame rather than grow, which is exactly
                // the stale-tick-is-worthless behavior we want here.
                let _ = ch.write(true, &frame);
            }
            // Mutate -> drain, per the single-mutation invariant.
            out.extend(self.drain());
        }
        Ok(out)
    }

    /// Write a whole payload to the reliable channel (caller owns framing; the
    /// event layer does not fragment on this channel — SCTP orders + resends).
    pub fn send_reliable(&mut self, payload: &[u8]) -> Result<Vec<Outbound>, RtcPlaneError> {
        let cid = self
            .ch_reliable
            .ok_or(RtcPlaneError::ChannelNotOpen(LABEL_RELIABLE.into()))?;
        let mut out = Vec::new();
        if let Some(mut ch) = self.rtc.channel(cid) {
            ch.write(true, payload)
                .map_err(|e| RtcPlaneError::Transport(format!("reliable write: {e:?}")))?;
        }
        out.extend(self.drain());
        Ok(out)
    }

    /// Drain all queued engine output to the next timeout, mapping it to
    /// [`Outbound`] and updating session state as side effects.
    fn drain(&mut self) -> Vec<Outbound> {
        let mut out = Vec::new();
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Transmit(t)) => {
                    out.push(Outbound::Datagram {
                        destination: t.destination,
                        contents: t.contents.to_vec(),
                    });
                }
                Ok(Output::Event(event)) => {
                    if let Some(mapped) = self.map_event(event) {
                        out.extend(mapped);
                    }
                }
                Ok(Output::Timeout(now)) => {
                    self.next_timeout = now;
                    break;
                }
                Err(_) => {
                    // Engine is dead (disconnect / protocol error). Stop driving.
                    self.next_timeout = Instant::now() + Duration::from_secs(3600);
                    out.push(Outbound::Event(SessionEvent::Closed));
                    break;
                }
            }
        }
        out
    }

    fn map_event(&mut self, event: Event) -> Option<Vec<Outbound>> {
        match event {
            Event::Connected => {
                self.connected = true;
                Some(vec![Outbound::Event(SessionEvent::Connected)])
            }
            Event::ChannelOpen(id, label) => {
                self.channel_labels.insert(id, label.clone());
                if label == LABEL_UNRELIABLE {
                    self.ch_unreliable = Some(id);
                } else if label == LABEL_RELIABLE {
                    self.ch_reliable = Some(id);
                }
                Some(vec![Outbound::Event(SessionEvent::ChannelOpen { id, label })])
            }
            Event::ChannelClose(id) => {
                if self.ch_unreliable == Some(id) {
                    self.ch_unreliable = None;
                }
                if self.ch_reliable == Some(id) {
                    self.ch_reliable = None;
                }
                Some(vec![Outbound::Event(SessionEvent::ChannelClose { id })])
            }
            Event::ChannelData(data) => Some(vec![Outbound::Data {
                channel: data.id,
                binary: data.binary,
                payload: data.data,
            }]),
            Event::Closed => Some(vec![Outbound::Event(SessionEvent::Closed)]),
            _ => None,
        }
    }

    /// Resolve a channel id back to its game role (for uplink routing).
    pub fn channel_role(&self, id: ChannelId) -> Option<PlaneChannel> {
        if self.ch_unreliable == Some(id) {
            Some(PlaneChannel::Unreliable)
        } else if self.ch_reliable == Some(id) {
            Some(PlaneChannel::Reliable)
        } else {
            self.channel_labels.get(&id).and_then(|label| {
                if label == LABEL_UNRELIABLE {
                    Some(PlaneChannel::Unreliable)
                } else if label == LABEL_RELIABLE {
                    Some(PlaneChannel::Reliable)
                } else {
                    None
                }
            })
        }
    }
}
