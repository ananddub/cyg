//! The lane contract as `src/server.rs` consumes it: `WebRtcServer::poll_inbound()`.
//!
//! The game loop used to match inbound payloads against their opcode alone, which meant a
//! 4-byte ack arriving on `game-unreliable` was believed even though that lane cannot
//! prove delivery. `poll_inbound` classifies each frame against the channel it came in on,
//! so the host's job is now to route `Inbound` variants — and this is the only place that
//! contract is exercised over a live SCTP association rather than a synthetic buffer.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use engine_net::transport::webrtc_frames::{Inbound, Lane};
use engine_net::transport::webrtc_server::{WebRtcServer, WebRtcServerEvent};
use engine_webrtc_protocol::{
    DeltaHeader, EVENT_WIRE_LEN, EventFrame, EventKind, FragmentAssembler, InputFrame,
    MAX_DELTA_BODY, MAX_DELTA_CHUNK, MAX_FRAME, AckKind, encode_event, encode_input, write_ack,
};
use str0m::change::SdpAnswer;
use str0m::channel::{ChannelConfig, Reliability};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const CLIENT_ADDR: &str = "127.0.0.1:50133";
const SERVER_ADDR: &str = "127.0.0.1:7781";
const PLAYER: &str = "lane_player";

/// The two lanes exactly as the browser SDK opens them: `game-unreliable` is unordered
/// with no retransmit budget, `game-reliable` is ordered and never dropped. A channel
/// whose config contradicts its label is a contract mismatch the server warns about, so
/// getting this wrong makes the test noisy instead of wrong.
fn channel(label: &str, lane: Lane) -> ChannelConfig {
    ChannelConfig {
        label: label.to_string(),
        ordered: lane == Lane::Reliable,
        reliability: match lane {
            Lane::Reliable => Reliability::Reliable,
            Lane::Unreliable => Reliability::MaxRetransmits { retransmits: 0 },
        },
        negotiated: None,
        protocol: String::new(),
    }
}

struct Pair {
    client: Rtc,
    server: WebRtcServer,
    client_addr: SocketAddr,
    server_addr: SocketAddr,
    unreliable: Option<str0m::channel::ChannelId>,
    reliable: Option<str0m::channel::ChannelId>,
    client_rx: HashMap<String, Vec<Vec<u8>>>,
    now: Instant,
}

impl Pair {
    fn connect() -> Pair {
        let now = Instant::now();
        let client_addr: SocketAddr = CLIENT_ADDR.parse().unwrap();
        let server_addr: SocketAddr = SERVER_ADDR.parse().unwrap();

        let mut client = Rtc::builder().build(now);
        let client_cand = Candidate::host(client_addr, Protocol::Udp).unwrap();
        client.add_local_candidate(client_cand.clone()).unwrap();

        let mut change = client.sdp_api();
        change.add_channel_with_config(channel("game-unreliable", Lane::Unreliable));
        change.add_channel_with_config(channel("game-reliable", Lane::Reliable));
        let (offer, pending) = change.apply().unwrap();

        let mut server = WebRtcServer::new(vec![server_addr]);
        let answer = server.handle_offer(PLAYER, &offer.to_sdp_string()).expect("offer");
        client
            .sdp_api()
            .accept_answer(pending, SdpAnswer::from_sdp_string(&answer).unwrap())
            .expect("answer");

        client.add_remote_candidate(Candidate::host(server_addr, Protocol::Udp).unwrap());
        server.handle_candidate(PLAYER, &client_cand.to_sdp_string()).unwrap();

        let mut pair = Pair {
            client,
            server,
            client_addr,
            server_addr,
            unreliable: None,
            reliable: None,
            client_rx: HashMap::new(),
            now,
        };
        for _ in 0..400 {
            pair.pump();
            if pair.server.is_connected(PLAYER)
                && pair.unreliable.is_some()
                && pair.reliable.is_some()
            {
                return pair;
            }
        }
        panic!("handshake never completed");
    }

    /// One time step of network in both directions.
    fn pump(&mut self) {
        self.now += Duration::from_millis(5);
        let _ = self.client.handle_input(Input::Timeout(self.now));
        self.server.tick(self.now);

        loop {
            match self.client.poll_output() {
                Ok(Output::Transmit(t)) => {
                    self.server.handle_udp(self.client_addr, self.server_addr, &t.contents)
                }
                Ok(Output::Event(Event::ChannelOpen(cid, label))) => {
                    if label.contains("unreliable") {
                        self.unreliable = Some(cid);
                    } else if label.contains("reliable") {
                        self.reliable = Some(cid);
                    }
                }
                Ok(Output::Event(Event::ChannelData(d))) => {
                    let label = match (self.unreliable, self.reliable) {
                        (Some(u), _) if d.id == u => "game-unreliable",
                        (_, Some(r)) if d.id == r => "game-reliable",
                        _ => "unknown",
                    };
                    self.client_rx.entry(label.to_string()).or_default().push(d.data.to_vec());
                }
                Ok(Output::Timeout(_)) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }

        for (dest, packet) in self.server.poll_transmits() {
            if let Ok(recv) = Receive::new(Protocol::Udp, self.server_addr, dest, &packet) {
                let _ = self.client.handle_input(Input::Receive(self.now, recv));
            }
        }
    }

    fn write_on(&mut self, lane: Lane, data: &[u8]) {
        let cid = match lane {
            Lane::Unreliable => self.unreliable.expect("unreliable open"),
            Lane::Reliable => self.reliable.expect("reliable open"),
        };
        let queued = self
            .client
            .channel(cid)
            .expect("channel")
            .write(true, data)
            .expect("write");
        assert!(queued, "{lane:?} accepted the frame");
    }

    /// Hand a buffer to the browser side of a lane, then drain the server's classified
    /// inbound the way the game loop does each tick.
    fn deliver(&mut self, lane: Lane, data: &[u8]) -> Vec<Inbound> {
        self.write_on(lane, data);
        for _ in 0..40 {
            self.pump();
            let got = self.server.poll_inbound();
            if !got.is_empty() {
                return got.into_iter().map(|(_, inbound)| inbound).collect();
            }
        }
        panic!("nothing arrived on {lane:?}");
    }

    /// Pump until the client's unreliable lane has produced every fragment of one delta.
    fn reassemble(&mut self, expect_seq: u16, expect_tick: u32) -> Option<Vec<u8>> {
        let mut assembler = FragmentAssembler::default();
        for _ in 0..120 {
            self.pump();
            for frame in self.client_rx.remove("game-unreliable").unwrap_or_default() {
                assert!(frame.len() <= MAX_FRAME, "a fragment must fit one datagram");
                let (header, chunk) = DeltaHeader::parse(&frame).expect("a stamped delta");
                assert_eq!(header.seq, expect_seq);
                assert_eq!(header.tick, expect_tick);
                assert!(chunk.len() <= MAX_DELTA_CHUNK);
                if let Some(done) = assembler.insert(&header, chunk, 0) {
                    assert_eq!(done.seq, expect_seq);
                    return Some(done.body);
                }
            }
            if assembler.assembled() > 0 {
                break;
            }
        }
        None
    }
}

/// The frame the browser sends 50–128 times a second has to reach `apply_input` with its
/// 20-byte guest payload intact — that blob is what the wasm room consumes.
#[test]
fn an_input_arrives_with_its_guest_payload_and_watermark() {
    let mut pair = Pair::connect();
    let frame = InputFrame {
        seq: 4217,
        flags: 0x0000_0011,
        aim_yaw: 0.25,
        aim_pitch: -0.5,
        move_x: 1.0,
        move_z: -1.0,
    };
    let mut wire = Vec::new();
    encode_input(&frame, &mut wire);

    let inbound = pair.deliver(Lane::Unreliable, &wire);
    let Inbound::Input { seq, payload, resupplied } = &inbound[0] else {
        panic!("expected an input, got {inbound:?}");
    };
    assert_eq!(*seq, 4217);
    assert!(!*resupplied, "the first sighting of a seq is never a resupply");
    assert_eq!(&payload[..4], &frame.flags.to_le_bytes());
    assert_eq!(&payload[4..8], &frame.aim_yaw.to_le_bytes());
    assert_eq!(&payload[16..20], &frame.move_z.to_le_bytes());

    // SCTP redelivers, and a client that reconnects replays. The host keeps the frame —
    // rollback wants it — but must know it is not news.
    let again = pair.deliver(Lane::Unreliable, &wire);
    let Inbound::Input { resupplied, .. } = &again[0] else {
        panic!("expected the replayed input, got {again:?}");
    };
    assert!(*resupplied, "a seq the watermark already passed is flagged, not dropped");
}

/// The ack is the only proof the server has that a reliable event landed. Trusting one
/// that arrived on the unordered lane is trusting a message SCTP may have discarded — so
/// the transport refuses it and it never reaches the game at all.
#[test]
fn an_ack_is_only_believed_on_the_reliable_lane() {
    let mut pair = Pair::connect();
    let mut ack = Vec::new();
    write_ack(&mut ack, 91, AckKind::Event);

    let inbound = pair.deliver(Lane::Reliable, &ack);
    assert!(
        matches!(&inbound[0], Inbound::Ack(a) if a.seq == 91),
        "a reliable-lane ack is a delivery record: {inbound:?}"
    );

    let before = pair.server.stats().frames_misrouted;
    pair.write_on(Lane::Unreliable, &ack);
    for _ in 0..40 {
        pair.pump();
        if pair.server.stats().frames_misrouted > before {
            break;
        }
    }
    assert_eq!(
        pair.server.stats().frames_misrouted,
        before + 1,
        "the same bytes on the unreliable lane are refused"
    );
    assert!(
        pair.server.poll_inbound().is_empty(),
        "a refused frame must not be handed to the host, even as Inbound::Refused"
    );
}

/// An input pushed onto the ordered lane would stall every event behind it, so it is
/// refused rather than silently served.
#[test]
fn an_input_on_the_reliable_lane_is_refused() {
    let mut pair = Pair::connect();
    let mut wire = Vec::new();
    encode_input(
        &InputFrame { seq: 1, flags: 0, aim_yaw: 0.0, aim_pitch: 0.0, move_x: 0.0, move_z: 0.0 },
        &mut wire,
    );
    let before = pair.server.stats().frames_misrouted;
    pair.write_on(Lane::Reliable, &wire);
    for _ in 0..40 {
        pair.pump();
        if pair.server.stats().frames_misrouted > before {
            break;
        }
    }
    assert_eq!(pair.server.stats().frames_misrouted, before + 1);
    assert!(pair.server.poll_inbound().is_empty());
}

/// Gameplay events are opaque to the transport: it stamps nothing and drops nothing,
/// because the room assigns its own sequence when it fans the frame out.
#[test]
fn an_event_crosses_the_ordered_lane_byte_for_byte() {
    let mut pair = Pair::connect();
    let event = EventFrame {
        seq: 0,
        kind: EventKind::Fire,
        from: 7,
        target: u16::MAX,
        tick: 1234,
        value: 1.0,
    };
    let mut wire = Vec::with_capacity(EVENT_WIRE_LEN);
    encode_event(&event, &mut wire);

    let inbound = pair.deliver(Lane::Reliable, &wire);
    let Inbound::Event { raw } = &inbound[0] else { panic!("expected an event: {inbound:?}") };
    assert_eq!(raw, &wire, "the transport must not rewrite an event frame");
    assert_eq!(EventFrame::decode(raw).expect("still decodable").kind, EventKind::Fire);
}

/// What `src/server.rs` now calls once per player per tick: the transport owns the
/// stamp-and-split, and `delivered()` is the whole contract — anything else means this
/// client did not get this tick and the WebSocket fallback has to carry it.
#[test]
fn the_transports_own_split_delivers_one_reassembled_delta() {
    let mut pair = Pair::connect();
    let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();

    let report = pair.server.send_delta(PLAYER, 4097, 2008, &body).expect("fits the channel");
    assert!(report.fragments > 1, "a 5 KB delta must actually be split");
    assert!(report.delivered(), "the lane is open, so every frame is accepted");

    let assembled = pair.reassemble(4097, 2008);
    assert_eq!(assembled.as_deref(), Some(body.as_slice()));

    // A body past the fragment ceiling is refused as a whole, so the caller's fallback is
    // the only copy the client gets — never a truncated world state.
    let fat = vec![0u8; MAX_DELTA_BODY + 1];
    assert!(pair.server.send_delta(PLAYER, 1, 1, &fat).is_err());
    assert_eq!(pair.server.stats().deltas_refused_oversized, 1);
}

/// Every outbound frame this plane produces is one datagram. A single oversized message
/// is what turns a delta into an SCTP reassembly the unordered lane cannot guarantee.
#[test]
fn no_outbound_frame_exceeds_the_path_mtu() {
    let mut pair = Pair::connect();
    let body: Vec<u8> = (0..9000u32).map(|i| (i % 97) as u8).collect();
    let report = pair.server.send_delta(PLAYER, 7, 7, &body).expect("fits");
    assert_eq!(report.frames_dropped, 0);
    for (dest, packet) in pair.server.poll_transmits() {
        assert!(packet.len() <= MAX_FRAME, "a WebRTC UDP packet to {dest} is a whole frame");
    }
    // Events still fan out reliably, and never as deltas.
    let mut ev = Vec::new();
    encode_event(
        &EventFrame { seq: 3, kind: EventKind::Hit, from: 1, target: 2, tick: 88, value: 25.0 },
        &mut ev,
    );
    assert_eq!(pair.server.broadcast_reliable(&ev), 1);
    for (dest, packet) in pair.server.poll_transmits() {
        assert!(packet.len() <= MAX_FRAME, "reliable output to {dest} stays one frame");
    }
}

/// `poll_events` still reports the same bytes, so a host that reads both drains would
/// apply every input twice. `src/server.rs` reads `poll_inbound` and ignores `Message`.
#[test]
fn poll_events_still_carries_bytes_for_hosts_that_parse_themselves() {
    let mut pair = Pair::connect();
    let mut wire = Vec::new();
    encode_input(
        &InputFrame { seq: 99, flags: 3, aim_yaw: 0.0, aim_pitch: 0.0, move_x: 0.0, move_z: 0.0 },
        &mut wire,
    );
    pair.write_on(Lane::Unreliable, &wire);
    let mut saw_message = false;
    for _ in 0..40 {
        pair.pump();
        for ev in pair.server.poll_events() {
            if matches!(ev, WebRtcServerEvent::Message { .. }) {
                saw_message = true;
            }
        }
        if saw_message {
            break;
        }
    }
    assert!(saw_message, "both drains see the frame; the host picks exactly one");
}
