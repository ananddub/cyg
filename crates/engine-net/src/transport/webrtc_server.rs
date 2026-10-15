//! # WebRTC Server Transport (Sans-IO via str0m)
//!
//! Direct Browser-to-Server WebRTC DataChannels over UDP (Star Topology):
//! - `game-unreliable`: Unordered, 0-retransmits high-frequency channel for 50Hz–128Hz input & state deltas.
//! - `game-reliable`: Ordered reliable channel for critical game events (fire, damage, kills).
//!
//! Uses `str0m` with zero async runtime overhead, integrating directly into `mio` event loops.
//!
//! ## The channel contract, and why it is not guessed
//!
//! A label is the only thing that names a lane, so the mapping is exact:
//! [`webrtc_frames::classify_label`] accepts `game-unreliable` and
//! `game-reliable` verbatim, tolerates a documented legacy alias with a
//! one-time warning, and ignores everything else. An unknown channel is never
//! folded into the reliable lane — that would hand a debug or telemetry socket
//! the reliability guarantee gameplay events depend on. Because a label can
//! lie, the SCTP parameters the peer actually negotiated are checked against
//! the lane too, and the mismatch is reported in [`WebRtcServer::stats`] rather
//! than papered over.
//!
//! Reliability is explicit in both directions: a delta body is stamped and
//! split to the path MTU before it leaves ([`WebRtcServer::send_delta`]), and an
//! inbound frame is only honoured on the lane whose reliability makes it
//! meaningful — a 4-byte ack pattern that arrived over a 0-retransmit channel
//! is not proof of delivery, so it is refused and counted instead of believed.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::Instant;

use str0m::change::SdpOffer;
use str0m::channel::ChannelId;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

use super::webrtc_frames::{
    DeltaTooLarge, FRAME_MAX_WIRE_LEN, Inbound, LabelVerdict, Lane, MAX_QUEUED_FRAMES,
    OutboundDelta, OutboundQueue, RejectReason, SeqWatermark, classify_inbound, classify_label,
    contract_mismatch, split_delta,
};

/// Events emitted by the WebRTC server to the game engine loop.
#[derive(Debug)]
pub enum WebRtcServerEvent {
    Connected {
        client_id: String,
    },
    Disconnected {
        client_id: String,
    },
    ChannelOpen {
        client_id: String,
        label: String,
    },
    Message {
        client_id: String,
        channel_label: String,
        data: Vec<u8>,
    },
}

/// Contract counters for one transport instance. Every field distinguishes
/// "the room is quiet" from "the transport is throwing gameplay away".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportStats {
    /// Frames handed to SCTP for transmission.
    pub frames_sent: u64,
    /// Frames held in a lane queue because SCTP's send buffer was full.
    pub frames_queued: u64,
    /// Deltas whose every frame reached the lane (written or queued).
    pub deltas_sent: u64,
    /// Deltas refused because the body needed more than `MAX_FRAGMENTS` chunks.
    pub deltas_refused_oversized: u64,
    /// Deltas discarded under backpressure, oldest first.
    pub deltas_dropped_saturated: u64,
    /// Frames discarded with those deltas.
    pub frames_dropped_saturated: u64,
    /// Queued deltas replaced by a newer one on the unreliable lane.
    pub deltas_superseded: u64,
    /// Writes bigger than the path MTU, refused instead of truncated.
    pub frames_refused_oversize: u64,
    /// Frames discarded because the lane had no open channel.
    pub frames_dropped_no_channel: u64,
    /// `str0m` write errors on a channel id that was open.
    pub write_errors: u64,
    pub inputs_received: u64,
    /// Inputs whose seq the high-water mark had already passed. Still reported
    /// to the host, because rollback replay wants genuinely late inputs rather
    /// than dropping them.
    pub inputs_stale: u64,
    pub events_received: u64,
    pub acks_received: u64,
    /// Acks that did not move the forward-only high-water mark.
    pub acks_stale: u64,
    /// Frames refused for arriving on the other lane.
    pub frames_misrouted: u64,
    /// Frames refused for a bad length or impossible geometry.
    pub frames_malformed: u64,
    /// Traffic on a channel that is not part of the contract.
    pub frames_off_contract: u64,
    /// Channels mapped through a legacy alias label.
    pub legacy_alias_channels: u64,
    /// Channels whose negotiated SCTP reliability does not match their lane.
    pub contract_violations: u64,
}

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
    legacy_alias_warned: bool,
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

    fn channel_id(&self, lane: Lane) -> Option<ChannelId> {
        match lane {
            Lane::Unreliable => self.ch_unreliable,
            Lane::Reliable => self.ch_reliable,
        }
    }

    fn queue(&mut self, lane: Lane) -> &mut OutboundQueue {
        &mut self.outbound[lane as usize]
    }

    fn queue_len(&self, lane: Lane) -> usize {
        self.outbound[lane as usize].frames()
    }

    /// Throw away everything held for this session's lanes and report how many
    /// frames left: a session being replaced or removed can never deliver them.
    fn discard_outbound(&mut self) -> usize {
        self.outbound.iter_mut().map(|q| q.clear()).sum()
    }
}

/// Everything a session's `str0m` output can produce. One field so it can be
/// borrowed alongside `sessions` — field-level split borrows do not extend
/// through a method call on `&mut self`.
#[derive(Default)]
struct Sink {
    events: Vec<WebRtcServerEvent>,
    inbound: Vec<(String, Inbound)>,
    pending: Vec<(SocketAddr, Vec<u8>)>,
    stats: TransportStats,
}

/// The WebRTC Server engine managing multiple client RTC sessions over UDP.
pub struct WebRtcServer {
    sessions: HashMap<String, WebRtcSession>,
    addr_to_session: HashMap<SocketAddr, String>,
    announced_candidates: Vec<Candidate>,
    sink: Sink,
}

/// Outcome of one attempt to hand a frame to `str0m`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteAttempt {
    Accepted,
    /// SCTP's send buffer is full: hold the rest and retry on the next drain.
    Saturated,
    /// The channel is gone; nothing queued behind it can be delivered either.
    Lost,
}

impl WebRtcServer {
    pub fn new(announced_addrs: Vec<SocketAddr>) -> Self {
        let mut announced_candidates = Vec::new();
        for addr in announced_addrs {
            if let Ok(cand) = Candidate::host(addr, Protocol::Udp) {
                announced_candidates.push(cand);
            }
        }
        Self {
            sessions: HashMap::new(),
            addr_to_session: HashMap::new(),
            announced_candidates,
            sink: Sink::default(),
        }
    }

    /// Add an announced public or LAN host candidate address.
    pub fn add_announced_addr(&mut self, addr: SocketAddr) {
        if let Ok(cand) = Candidate::host(addr, Protocol::Udp) {
            self.announced_candidates.push(cand);
        }
    }

    /// Process an SDP Offer received from a client via WebSocket signaling.
    /// Creates a dedicated `str0m::Rtc` instance and returns the SDP Answer.
    pub fn handle_offer(&mut self, client_id: &str, sdp: &str) -> Result<String, String> {
        let offer = SdpOffer::from_sdp_string(sdp).map_err(|e| format!("invalid SDP offer: {e:?}"))?;
        let now = Instant::now();

        let mut rtc = Rtc::builder().build(now);
        for cand in &self.announced_candidates {
            let _ = rtc.add_local_candidate(cand.clone());
        }

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("failed to accept offer: {e:?}"))?;

        let answer_sdp = answer.to_sdp_string();

        // A re-offer replaces the session, and the old one's address mapping has
        // to go with it: a stale entry keeps routing this client's packets into
        // an abandoned `Rtc` whose channels are closed.
        if let Some(mut old) = self.sessions.remove(client_id) {
            if let Some(addr) = old.remote_addr {
                self.addr_to_session.remove(&addr);
            }
            self.sink.stats.frames_dropped_no_channel += old.discard_outbound() as u64;
        }

        let mut session = WebRtcSession::new(client_id.to_string(), rtc);
        Self::poll_session_events(&mut session, &mut self.sink);

        self.sessions.insert(client_id.to_string(), session);
        Ok(answer_sdp)
    }

    /// Handle an ICE candidate sent by the client.
    pub fn handle_candidate(&mut self, client_id: &str, sdp_candidate: &str) -> Result<(), String> {
        let Some(session) = self.sessions.get_mut(client_id) else {
            return Err("session not found".to_string());
        };

        let trimmed = sdp_candidate.trim();
        let cand = Candidate::from_sdp_string(trimmed)
            .or_else(|_| {
                let stripped = trimmed.trim_start_matches("a=").trim();
                Candidate::from_sdp_string(stripped)
            })
            .or_else(|_| {
                let stripped = trimmed.trim_start_matches("a=").trim_start_matches("candidate:").trim();
                Candidate::from_sdp_string(stripped)
            });

        if let Ok(c) = cand {
            let _ = session.rtc.add_remote_candidate(c);
            Self::poll_session_events(session, &mut self.sink);
        }
        Ok(())
    }

    /// Ingest an incoming UDP packet received on the server's UDP socket.
    pub fn handle_udp(&mut self, from: SocketAddr, mut local: SocketAddr, packet: &[u8]) {
        let now = Instant::now();

        // If socket was bound to 0.0.0.0, resolve local destination to matching announced candidate
        if local.ip().is_unspecified() {
            if from.ip().is_loopback() {
                local = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), local.port());
            } else if let Some(cand) = self.announced_candidates.iter().find(|c| !c.addr().ip().is_loopback()) {
                local = cand.addr();
            } else if let Some(cand) = self.announced_candidates.first() {
                local = cand.addr();
            }
        }

        let Ok(recv) = Receive::new(Protocol::Udp, from, local, packet) else {
            return;
        };
        let input = Input::Receive(now, recv);

        // 1. Fast path: check known remote address mapping
        if let Some(client_id) = self.addr_to_session.get(&from) {
            if let Some(session) = self.sessions.get_mut(client_id) {
                session.last_active = now;
                let _ = session.rtc.handle_input(input);
                Self::poll_session_events(session, &mut self.sink);
                return;
            }
        }

        // 2. Slow path: check which RTC session accepts this packet (ICE / STUN binding)
        let mut matched_client_id: Option<String> = None;
        for (client_id, session) in &mut self.sessions {
            if session.rtc.accepts(&input) {
                session.last_active = now;
                session.remote_addr = Some(from);
                let _ = session.rtc.handle_input(input);
                Self::poll_session_events(session, &mut self.sink);
                matched_client_id = Some(client_id.clone());
                break;
            }

            // Fallback: try all announced candidates as destination
            let mut accepted = false;
            for cand in &self.announced_candidates {
                if let Ok(cand_recv) = Receive::new(Protocol::Udp, from, cand.addr(), packet) {
                    let cand_input = Input::Receive(now, cand_recv);
                    if session.rtc.accepts(&cand_input) {
                        session.last_active = now;
                        session.remote_addr = Some(from);
                        let _ = session.rtc.handle_input(cand_input);
                        Self::poll_session_events(session, &mut self.sink);
                        matched_client_id = Some(client_id.clone());
                        accepted = true;
                        break;
                    }
                }
            }
            if accepted {
                break;
            }
        }

        if let Some(client_id) = matched_client_id {
            self.addr_to_session.insert(from, client_id);
        }
    }

    /// Periodic tick for DTLS timeouts and SCTP retransmissions. Also the retry
    /// point for anything a full send buffer made a session hold.
    pub fn tick(&mut self, now: Instant) {
        for session in self.sessions.values_mut() {
            let should_tick = match session.next_timeout {
                Some(t) => now >= t,
                None => false,
            };
            if should_tick {
                let _ = session.rtc.handle_input(Input::Timeout(now));
                Self::poll_session_events(session, &mut self.sink);
            }
        }
        self.flush_outbound();
    }

    /// Stamp, fragment and send one world delta on `game-unreliable`.
    ///
    /// The caller owns `seq` — the room-wide delta counter, identical for every
    /// recipient of one tick — and `tick`. A body that needs more than
    /// `MAX_FRAGMENTS` chunks is refused as a whole and counted: never
    /// truncated, because a partial delta is a corrupt world state and an
    /// oversized frame is a message SCTP loses entirely.
    ///
    /// Frames SCTP will not take are queued for this session under the
    /// newest-wins rule, so the poll loop never blocks on one slow client.
    pub fn send_delta(
        &mut self,
        client_id: &str,
        seq: u16,
        tick: u32,
        body: &[u8],
    ) -> Result<DeltaSendReport, DeltaTooLarge> {
        let mut frames = Vec::new();
        let fragments = match split_delta(seq, tick, body, &mut frames) {
            Ok(fragments) => fragments,
            Err(err) => {
                self.sink.stats.deltas_refused_oversized += 1;
                return Err(err);
            }
        };
        let Some(session) = self.sessions.get_mut(client_id) else {
            self.sink.stats.frames_dropped_no_channel += fragments as u64;
            return Ok(DeltaSendReport::skipped(fragments));
        };
        Ok(Self::send_delta_frames(session, seq, tick, frames, fragments, &mut self.sink))
    }

    /// Send one stamped delta to every session with an open unreliable lane.
    /// Returns the number of sessions that received all of its frames.
    ///
    /// `seq` is room-wide, so the frames are identical for every recipient and
    /// the split is done once.
    pub fn broadcast_delta(&mut self, seq: u16, tick: u32, body: &[u8]) -> usize {
        let mut prototype = Vec::new();
        let fragments = match split_delta(seq, tick, body, &mut prototype) {
            Ok(fragments) => fragments,
            Err(_) => {
                self.sink.stats.deltas_refused_oversized += 1;
                return 0;
            }
        };
        let mut served = 0;
        for session in self.sessions.values_mut() {
            let report = Self::send_delta_frames(
                session,
                seq,
                tick,
                prototype.clone(),
                fragments,
                &mut self.sink,
            );
            if report.delivered() {
                served += 1;
            }
        }
        served
    }

    /// Send one already-framed message on the unreliable lane (`game-unreliable`).
    ///
    /// This is the raw lane primitive; prefer [`Self::send_delta`] for world
    /// state, which stamps and fragments for you. A message bigger than the
    /// path MTU is refused and counted rather than truncated or handed to SCTP
    /// for its own reassembly.
    pub fn send_unreliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        self.send_frame(Lane::Unreliable, client_id, data)
    }

    /// Send one already-framed message on the reliable lane (`game-reliable`).
    pub fn send_reliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        self.send_frame(Lane::Reliable, client_id, data)
    }

    /// Broadcast data to all clients with an open `game-unreliable` channel.
    pub fn broadcast_unreliable(&mut self, data: &[u8]) -> usize {
        self.broadcast_frame(Lane::Unreliable, data)
    }

    /// Broadcast data to all clients with an open `game-reliable` channel.
    pub fn broadcast_reliable(&mut self, data: &[u8]) -> usize {
        self.broadcast_frame(Lane::Reliable, data)
    }

    /// The newest input seq seen from this client, wrap-safe and forward-only.
    /// This is what the host echoes back as `acked_seq`.
    pub fn last_input_seq(&self, client_id: &str) -> Option<u16> {
        self.sessions.get(client_id).and_then(|s| s.last_input_seq.get())
    }

    /// The newest event seq this client has acked on the reliable lane.
    pub fn acked_event_seq(&self, client_id: &str) -> Option<u16> {
        self.sessions.get(client_id).and_then(|s| s.acked_event_seq.get())
    }

    /// Frames both lanes are still holding for this client: a slow-client
    /// signal a host can meter without walking the sessions itself.
    pub fn outbound_backlog(&self, client_id: &str) -> usize {
        self.sessions
            .get(client_id)
            .map(|s| s.queue_len(Lane::Unreliable) + s.queue_len(Lane::Reliable))
            .unwrap_or(0)
    }

    /// Check if a client is connected over WebRTC.
    pub fn is_connected(&self, client_id: &str) -> bool {
        self.sessions
            .get(client_id)
            .map(|s| s.connected && s.ch_unreliable.is_some())
            .unwrap_or(false)
    }

    /// Contract counters, for metrics and for telling a dead transport apart
    /// from an idle room.
    pub fn stats(&self) -> TransportStats {
        self.sink.stats
    }

    /// Number of active WebRTC client sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Remove a client session.
    pub fn remove_session(&mut self, client_id: &str) {
        if let Some(mut session) = self.sessions.remove(client_id) {
            if let Some(addr) = session.remote_addr {
                self.addr_to_session.remove(&addr);
            }
            self.sink.stats.frames_dropped_no_channel += session.discard_outbound() as u64;
            self.sink.events.push(WebRtcServerEvent::Disconnected {
                client_id: client_id.to_string(),
            });
        }
    }

    /// Drain queued events for the game loop.
    pub fn poll_events(&mut self) -> Vec<WebRtcServerEvent> {
        std::mem::take(&mut self.sink.events)
    }

    /// Drain client frames the transport has already classified against their
    /// lane, with the high-water-mark result attached: inputs know whether they
    /// are resupplies, acks are already parsed. [`Self::poll_events`] still
    /// carries the same bytes for hosts that parse them themselves.
    pub fn poll_inbound(&mut self) -> Vec<(String, Inbound)> {
        std::mem::take(&mut self.sink.inbound)
    }

    /// Drain outgoing UDP datagrams to send via socket. Queued frames are
    /// retried first, so a lane that was full last tick drains in arrival order.
    pub fn poll_transmits(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        self.flush_outbound();
        std::mem::take(&mut self.sink.pending)
    }

    fn send_frame(&mut self, lane: Lane, client_id: &str, data: &[u8]) -> bool {
        if data.len() > FRAME_MAX_WIRE_LEN {
            self.sink.stats.frames_refused_oversize += 1;
            eprintln!(
                "[webrtc] ⚠️ refused {} B on {} — MTU budget is {} B, split it (send_delta)",
                data.len(),
                lane.label(),
                FRAME_MAX_WIRE_LEN
            );
            return false;
        }
        let Some(session) = self.sessions.get_mut(client_id) else {
            return false;
        };
        let sent = Self::write_or_queue(session, lane, data, None, None, &mut self.sink);
        Self::settle(session, &mut self.sink);
        sent
    }

    fn broadcast_frame(&mut self, lane: Lane, data: &[u8]) -> usize {
        if data.len() > FRAME_MAX_WIRE_LEN {
            self.sink.stats.frames_refused_oversize += 1;
            return 0;
        }
        let mut count = 0;
        for session in self.sessions.values_mut() {
            if Self::write_or_queue(session, lane, data, None, None, &mut self.sink) {
                count += 1;
            }
            Self::settle(session, &mut self.sink);
        }
        count
    }

    /// Retry every lane queue that still has an open channel. Each pass pops a
    /// whole delta and stops at the first lane that is still saturated, so one
    /// slow client cannot add unbounded work to a tick.
    fn flush_outbound(&mut self) {
        for session in self.sessions.values_mut() {
            for lane in [Lane::Unreliable, Lane::Reliable] {
                Self::drain_queue(session, lane, &mut self.sink);
            }
            Self::settle(session, &mut self.sink);
        }
    }

    /// Drain this session's `str0m` output queue.
    ///
    /// A `Channel::write` only lands in the SCTP association; the datagram
    /// exists once the output queue is polled. Skipping this makes every delta
    /// leave a tick late, and a host that sends then asks for transmits in the
    /// same iteration would get nothing back.
    fn settle(session: &mut WebRtcSession, sink: &mut Sink) {
        Self::poll_session_events(session, sink);
    }

    fn drain_queue(session: &mut WebRtcSession, lane: Lane, sink: &mut Sink) {
        let Some(cid) = session.channel_id(lane) else {
            if !session.queue(lane).is_empty() {
                let n = session.queue(lane).clear();
                sink.stats.frames_dropped_no_channel += n as u64;
            }
            return;
        };
        while let Some(delta) = session.queue(lane).pop() {
            let total = delta.frames.len();
            let mut written = 0usize;
            let mut saturated_at: Option<usize> = None;
            let mut lost = false;
            for (idx, frame) in delta.frames.iter().enumerate() {
                match Self::try_write(session, cid, frame) {
                    WriteAttempt::Accepted => {
                        sink.stats.frames_sent += 1;
                        written += 1;
                    }
                    WriteAttempt::Saturated => {
                        saturated_at = Some(idx);
                        break;
                    }
                    WriteAttempt::Lost => {
                        lost = true;
                        break;
                    }
                }
            }
            if written == total {
                continue;
            }
            match saturated_at {
                Some(idx) => {
                    // Put the undelivered tail back at the head and stop: the
                    // next drain must not skip past a frame still waiting.
                    let tail = VecDeque::from(delta.frames.into_iter().skip(idx).collect::<Vec<_>>());
                    session.queue(lane).push_front(OutboundDelta {
                        seq: delta.seq,
                        tick: delta.tick,
                        frames: tail,
                    });
                }
                None => {
                    let dropped = total - written;
                    if lost {
                        sink.stats.write_errors += 1;
                    }
                    sink.stats.frames_dropped_no_channel += dropped as u64;
                }
            }
            break;
        }
    }

    /// Write one frame, or hold it if the lane is full. `true` means the frame
    /// reached SCTP or this session's queue — not that it reached the client.
    fn write_or_queue(
        session: &mut WebRtcSession,
        lane: Lane,
        frame: &[u8],
        seq: Option<u16>,
        tick: Option<u32>,
        sink: &mut Sink,
    ) -> bool {
        let Some(cid) = session.channel_id(lane) else {
            sink.stats.frames_dropped_no_channel += 1;
            return false;
        };
        // Lane order is part of what the reliable channel promises, so a frame
        // that arrives while the lane is already holding frames joins the back
        // of the queue instead of overtaking it.
        if session.queue(lane).is_empty() {
            match Self::try_write(session, cid, frame) {
                WriteAttempt::Accepted => {
                    sink.stats.frames_sent += 1;
                    return true;
                }
                WriteAttempt::Lost => {
                    sink.stats.write_errors += 1;
                    sink.stats.frames_dropped_no_channel += 1;
                    return false;
                }
                WriteAttempt::Saturated => {}
            }
        }
        // A single frame always fits a queue of this capacity, so it is either
        // held here or it evicts something older: it is never silently lost.
        Self::enqueue(session, lane, seq, tick, vec![frame.to_vec()], sink);
        sink.stats.frames_queued += 1;
        true
    }

    /// Queue frames as one delta unit, counting what the cap forced out. Returns
    /// how many of *these* frames the queue is now holding.
    fn enqueue(
        session: &mut WebRtcSession,
        lane: Lane,
        seq: Option<u16>,
        tick: Option<u32>,
        frames: Vec<Vec<u8>>,
        sink: &mut Sink,
    ) -> usize {
        let count = frames.len();
        let (dropped_deltas, dropped_frames) = session.queue(lane).push(
            OutboundDelta {
                seq,
                tick,
                frames: VecDeque::from(frames),
            },
            MAX_QUEUED_FRAMES,
        );
        sink.stats.deltas_dropped_saturated += dropped_deltas;
        sink.stats.frames_dropped_saturated += dropped_frames;
        if count > MAX_QUEUED_FRAMES {
            0
        } else {
            count
        }
    }

    fn try_write(session: &mut WebRtcSession, cid: ChannelId, frame: &[u8]) -> WriteAttempt {
        let Some(mut channel) = session.rtc.channel(cid) else {
            return WriteAttempt::Lost;
        };
        match channel.write(true, frame) {
            // `Ok(false)` is `str0m`'s own buffer check refusing the message. It
            // is not success, and counting it as success is how a delta vanishes
            // while the host still believes it was framed and sent — and so skips
            // its reliable fallback.
            Ok(true) => WriteAttempt::Accepted,
            Ok(false) => WriteAttempt::Saturated,
            Err(_) => WriteAttempt::Lost,
        }
    }

    /// Send a whole split delta: write what the lane will take, queue the rest.
    fn send_delta_frames(
        session: &mut WebRtcSession,
        seq: u16,
        tick: u32,
        frames: Vec<Vec<u8>>,
        fragments: usize,
        sink: &mut Sink,
    ) -> DeltaSendReport {
        let lane = Lane::Unreliable;
        let Some(cid) = session.channel_id(lane) else {
            sink.stats.frames_dropped_no_channel += frames.len() as u64;
            return DeltaSendReport::skipped(fragments);
        };

        // A queued tick nobody wants any more is worse than an empty queue: it
        // delays the newest delta behind world state the client discards on
        // arrival. So the newest wins and older queued deltas leave first.
        let superseded = if session.queue(lane).is_empty() {
            0
        } else {
            session.queue(lane).purge_older_than(seq)
        };
        if superseded > 0 {
            sink.stats.deltas_superseded += superseded as u64;
        }

        let mut accepted = 0usize;
        let mut dropped = 0usize;
        // Frames already waiting means this delta joins the queue too, so a
        // partially delivered delta cannot interleave with the one before it.
        let mut hold_from: Option<usize> = if session.queue(lane).is_empty() { None } else { Some(0) };

        if hold_from.is_none() {
            for (idx, frame) in frames.iter().enumerate() {
                match Self::try_write(session, cid, frame) {
                    WriteAttempt::Accepted => {
                        sink.stats.frames_sent += 1;
                        accepted += 1;
                    }
                    WriteAttempt::Saturated => {
                        hold_from = Some(idx);
                        break;
                    }
                    WriteAttempt::Lost => {
                        sink.stats.write_errors += 1;
                        dropped = frames.len() - idx;
                        sink.stats.frames_dropped_no_channel += dropped as u64;
                        break;
                    }
                }
            }
        }

        if let Some(start) = hold_from {
            let tail = &frames[start..];
            let held = Self::enqueue(session, lane, Some(seq), Some(tick), tail.to_vec(), sink);
            accepted += held;
            sink.stats.frames_queued += held as u64;
            dropped += tail.len() - held;
        }

        Self::settle(session, sink);

        let report = DeltaSendReport {
            frames_accepted: accepted,
            frames_dropped: dropped,
            fragments,
            superseded_queued: superseded,
        };
        if report.delivered() {
            sink.stats.deltas_sent += 1;
        }
        report
    }

    fn poll_session_events(session: &mut WebRtcSession, sink: &mut Sink) {
        while let Ok(output) = session.rtc.poll_output() {
            match output {
                Output::Transmit(send) => {
                    sink.pending.push((send.destination, send.contents.to_vec()));
                }
                Output::Event(Event::Connected) => {
                    session.connected = true;
                    sink.events.push(WebRtcServerEvent::Connected {
                        client_id: session.client_id.clone(),
                    });
                }
                Output::Event(Event::ChannelOpen(cid, label)) => {
                    let verdict = classify_label(&label);
                    let legacy = matches!(verdict, LabelVerdict::LegacyAlias(_));
                    if let LabelVerdict::Contract(lane) | LabelVerdict::LegacyAlias(lane) = verdict {
                        match lane {
                            Lane::Unreliable => session.ch_unreliable = Some(cid),
                            Lane::Reliable => session.ch_reliable = Some(cid),
                        }
                        session.lanes.insert(cid, lane);
                        if legacy {
                            sink.stats.legacy_alias_channels += 1;
                            if !session.legacy_alias_warned {
                                session.legacy_alias_warned = true;
                                eprintln!(
                                    "[webrtc] ⚠️ client {} opened '{label}': not a contract label, mapped to {} as a legacy alias",
                                    session.client_id,
                                    lane.label()
                                );
                            }
                        }
                        // The label names a lane; the negotiated SCTP parameters
                        // decide what it is. Record the disagreement instead of
                        // trusting either one alone.
                        let mismatch = session
                            .rtc
                            .channel(cid)
                            .and_then(|c| c.config().cloned())
                            .and_then(|config| contract_mismatch(lane, &config));
                        if let Some(reason) = mismatch {
                            sink.stats.contract_violations += 1;
                            eprintln!("[webrtc] ⚠️ channel '{label}' on {}: {reason}", session.client_id);
                        }
                    }
                    // Anything else stays unmapped: it is neither writable as a
                    // game lane nor believed as a source of game frames.
                    sink.events.push(WebRtcServerEvent::ChannelOpen {
                        client_id: session.client_id.clone(),
                        label,
                    });
                }
                Output::Event(Event::ChannelData(data)) => {
                    let lane = session.lanes.get(&data.id).copied();
                    let mut inbound = classify_inbound(lane, &data.data);
                    if let Inbound::Input { seq, ref mut resupplied, .. } = inbound {
                        *resupplied = !session.last_input_seq.advance(seq);
                        sink.stats.inputs_received += 1;
                        if *resupplied {
                            sink.stats.inputs_stale += 1;
                        }
                    } else if let Inbound::Ack(ack) = inbound {
                        if session.acked_event_seq.advance(ack.seq) {
                            sink.stats.acks_received += 1;
                        } else {
                            sink.stats.acks_stale += 1;
                        }
                    } else if matches!(inbound, Inbound::Event { .. }) {
                        sink.stats.events_received += 1;
                    }

                    match inbound {
                        Inbound::Refused(reason) => match reason {
                            RejectReason::WrongLane { .. } => sink.stats.frames_misrouted += 1,
                            RejectReason::Malformed { .. } => sink.stats.frames_malformed += 1,
                            RejectReason::OffContract => sink.stats.frames_off_contract += 1,
                        },
                        other => {
                            let label = session
                                .channel_labels
                                .get(&data.id)
                                .cloned()
                                .unwrap_or_else(|| lane.map(Lane::label).unwrap_or("unknown").to_string());
                            sink.inbound.push((session.client_id.clone(), other));
                            sink.events.push(WebRtcServerEvent::Message {
                                client_id: session.client_id.clone(),
                                channel_label: label,
                                data: data.data,
                            });
                        }
                    }
                }
                Output::Event(Event::ChannelClose(cid)) => {
                    let lane = session.lanes.remove(&cid);
                    if session.ch_unreliable == Some(cid) {
                        session.ch_unreliable = None;
                    }
                    if session.ch_reliable == Some(cid) {
                        session.ch_reliable = None;
                    }
                    if let Some(lane) = lane {
                        // Whatever was waiting for it can never be delivered.
                        let n = session.queue(lane).clear();
                        sink.stats.frames_dropped_no_channel += n as u64;
                    }
                    session.channel_labels.remove(&cid);
                }
                Output::Timeout(t) => {
                    session.next_timeout = Some(t);
                    break;
                }
                _ => {}
            }
        }
    }
}

/// What one session's delta send actually achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaSendReport {
    /// Frames written to SCTP or accepted into this session's queue.
    pub frames_accepted: usize,
    /// Frames discarded because the lane was unusable or the queue could not
    /// hold them.
    pub frames_dropped: usize,
    /// How many frames this delta needed.
    pub fragments: usize,
    /// Older queued deltas this one replaced.
    pub superseded_queued: usize,
}

impl DeltaSendReport {
    fn skipped(fragments: usize) -> Self {
        Self {
            frames_accepted: 0,
            frames_dropped: fragments,
            fragments,
            superseded_queued: 0,
        }
    }

    /// Every frame reached the lane. Anything else means this client did not
    /// get this tick, and the host should use its reliable fallback — the same
    /// contract the previous `send_delta_framed` returned as `0`.
    pub fn delivered(&self) -> bool {
        self.frames_accepted == self.fragments && self.frames_dropped == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_webrtc_protocol::{MAX_DELTA_BODY, MAX_FRAME};

    const FAT: usize = MAX_DELTA_BODY + 1;

    /// The refusal happens at the codec boundary, before any session lookup, so
    /// an unsendable body is never partially written either.
    #[test]
    fn a_delta_needing_256_fragments_is_refused_and_counted() {
        let mut server = WebRtcServer::new(vec!["127.0.0.1:7791".parse().unwrap()]);
        let body = vec![0x7fu8; FAT];
        let err = server
            .send_delta("nobody", 1, 1, &body)
            .expect_err("more than MAX_FRAGMENTS cannot be carried");
        assert!(err.to_string().contains("fragments"));
        assert_eq!(server.stats().deltas_refused_oversized, 1);
        assert_eq!(server.stats().frames_sent, 0, "nothing reached a lane");
    }

    #[test]
    fn broadcast_counts_a_refused_delta_for_every_session_at_once() {
        let mut server = WebRtcServer::new(vec!["127.0.0.1:7792".parse().unwrap()]);
        assert_eq!(server.broadcast_delta(3, 4, &vec![0u8; FAT]), 0);
        assert_eq!(server.stats().deltas_refused_oversized, 1);
    }

    /// A legal delta with no session is reported as skipped, not as sent: the
    /// host must fall back rather than assume the client got its tick.
    #[test]
    fn a_delta_for_an_unknown_session_is_not_reported_as_delivered() {
        let mut server = WebRtcServer::new(vec![]);
        let report = server.send_delta("ghost", 9, 9, &[1, 2, 3]).unwrap();
        assert_eq!(report.fragments, 1);
        assert_eq!(report.frames_accepted, 0);
        assert!(!report.delivered());
        assert_eq!(server.stats().frames_dropped_no_channel, 1);
    }

    #[test]
    fn raw_writes_over_the_mtu_are_refused_not_truncated() {
        let mut server = WebRtcServer::new(vec![]);
        let too_big = vec![0u8; MAX_FRAME + 1];
        assert!(!server.send_unreliable("nobody", &too_big));
        assert!(!server.send_reliable("nobody", &too_big));
        assert_eq!(server.broadcast_unreliable(&too_big), 0);
        assert_eq!(server.broadcast_reliable(&too_big), 0);
        assert_eq!(server.stats().frames_refused_oversize, 4);
    }

    #[test]
    fn seq_watermarks_are_per_session_and_forward_only() {
        let server = WebRtcServer::new(vec![]);
        assert_eq!(server.last_input_seq("nobody"), None);
        assert_eq!(server.acked_event_seq("nobody"), None);
        assert_eq!(server.outbound_backlog("nobody"), 0);
        assert!(!server.is_connected("nobody"));
    }
}
