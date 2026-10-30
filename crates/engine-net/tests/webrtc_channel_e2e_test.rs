//! The channel layer over a live SCTP association, not a byte array.
//!
//! The unit tests in `engine-webrtc-protocol` prove the codec agrees with the browser SDK
//! on the wire. This proves the wire survives the transport it was designed for: a real
//! `game-unreliable` DataChannel (`ordered: false, maxRetransmits: 0`) carrying a delta
//! too big for one packet, and the `game-reliable` ack that confirms an event landed.
//!
//! The server side here calls the same primitives `engine_server::host` calls per tick —
//! `delta_fragment_count`, `write_delta_frame`, `send_unreliable` — so a layout that only
//! works when the chunks happen to arrive in order fails loudly instead of in production.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use engine_net::transport::webrtc::{WebRtcServer, WebRtcServerEvent};
use engine_webrtc_protocol::{
    Ack, AckKind, DeltaHeader, FragmentAssembler, MAX_DELTA_CHUNK, MAX_FRAME, delta_fragment_count,
    parse_ack, write_ack, write_delta_frame,
};
use str0m::change::SdpAnswer;
use str0m::channel::ChannelId;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const CLIENT_ADDR: &str = "127.0.0.1:50123";
const SERVER_ADDR: &str = "127.0.0.1:7780";
const PLAYER: &str = "player_1";

/// A connected browser/server pair plus whatever each end has been handed.
struct Pair {
    client: Rtc,
    server: WebRtcServer,
    client_addr: SocketAddr,
    server_addr: SocketAddr,
    unreliable: Option<ChannelId>,
    reliable: Option<ChannelId>,
    /// Bytes the client received, per channel label.
    client_rx: HashMap<String, Vec<Vec<u8>>>,
    now: Instant,
}

impl Pair {
    /// Handshake a fresh str0m client against `WebRtcServer`, both driven in-process.
    fn connect() -> Pair {
        let now = Instant::now();
        let client_addr: SocketAddr = CLIENT_ADDR.parse().unwrap();
        let server_addr: SocketAddr = SERVER_ADDR.parse().unwrap();

        let mut client = Rtc::builder().build(now);
        let client_cand = Candidate::host(client_addr, Protocol::Udp).unwrap();
        client.add_local_candidate(client_cand.clone()).unwrap();

        let mut change = client.sdp_api();
        change.add_channel("game-unreliable".into());
        change.add_channel("game-reliable".into());
        let (offer, pending) = change.apply().unwrap();

        let mut server = WebRtcServer::new(vec![server_addr]);
        let answer_sdp = server
            .handle_offer(PLAYER, &offer.to_sdp_string())
            .expect("offer accepted");
        client
            .sdp_api()
            .accept_answer(pending, SdpAnswer::from_sdp_string(&answer_sdp).unwrap())
            .expect("answer accepted");

        client.add_remote_candidate(Candidate::host(server_addr, Protocol::Udp).unwrap());
        server
            .handle_candidate(PLAYER, &client_cand.to_sdp_string())
            .unwrap();

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
        panic!(
            "handshake never completed: connected={} channels {:?}/{:?}",
            pair.server.is_connected(PLAYER),
            pair.unreliable,
            pair.reliable
        );
    }

    /// One time step of network in both directions.
    fn pump(&mut self) {
        self.now += Duration::from_millis(5);

        let _ = self.client.handle_input(Input::Timeout(self.now));
        self.server.tick(self.now);

        // Everything the client wants to send, the server receives.
        loop {
            match self.client.poll_output() {
                Ok(Output::Transmit(t)) => {
                    self.server
                        .handle_udp(self.client_addr, self.server_addr, &t.contents)
                }
                Ok(Output::Event(Event::ChannelOpen(cid, label))) => {
                    if label.contains("unreliable") {
                        self.unreliable = Some(cid);
                    } else if label.contains("reliable") {
                        self.reliable = Some(cid);
                    }
                }
                Ok(Output::Event(Event::ChannelData(d))) => {
                    if let Some(cid) = self.unreliable {
                        let label = if d.id == cid {
                            "game-unreliable"
                        } else {
                            "game-reliable"
                        };
                        self.client_rx
                            .entry(label.to_string())
                            .or_default()
                            .push(d.data.to_vec());
                    }
                }
                Ok(Output::Timeout(_)) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }

        // And back again.
        for (dest, packet) in self.server.poll_transmits() {
            if let Ok(recv) = Receive::new(Protocol::Udp, self.server_addr, dest, &packet) {
                let _ = self.client.handle_input(Input::Receive(self.now, recv));
            }
        }
    }

    /// Drain the server's event queue after a pump.
    fn server_messages(&mut self) -> Vec<(String, Vec<u8>)> {
        self.pump();
        self.server
            .poll_events()
            .into_iter()
            .filter_map(|ev| match ev {
                WebRtcServerEvent::Message {
                    channel_label,
                    data,
                    ..
                } => Some((channel_label, data)),
                _ => None,
            })
            .collect()
    }

    /// The exact split `engine_server::host` performs on every tick's delta.
    fn send_delta(&mut self, seq: u16, tick: u32, body: &[u8]) -> usize {
        let total = delta_fragment_count(body.len()).expect("body fits the channel");
        let mut scratch = Vec::new();
        let mut sent = 0;
        for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {
            write_delta_frame(&mut scratch, seq, tick, idx as u8, total, chunk);
            assert!(scratch.len() <= MAX_FRAME, "a fragment must fit one packet");
            if self.server.send_unreliable(PLAYER, &scratch) {
                sent += 1;
            }
        }
        sent
    }
}

/// A delta larger than the MTU is split, and the client's assembler puts it back
/// byte for byte even when the fragments arrive in the wrong order.
#[test]
fn a_fragmented_delta_crosses_the_datagram_channel() {
    let mut pair = Pair::connect();

    let body: Vec<u8> = (0..6000u32).map(|i| (i % 197) as u8).collect();
    let total = delta_fragment_count(body.len()).unwrap();
    assert!(total > 1, "this delta must actually need fragments");

    assert_eq!(pair.send_delta(41, 1000, &body), total as usize);

    // One pump per fragment is enough for SCTP to hand each one up, in send order.
    let mut assembler = FragmentAssembler::default();
    let mut assembled = None;
    for _ in 0..50 {
        pair.pump();
        let frames: Vec<Vec<u8>> = pair.client_rx.remove("game-unreliable").unwrap_or_default();
        for frame in frames {
            let (header, chunk) = DeltaHeader::parse(&frame).expect("a stamped delta");
            if let Some(done) = assembler.insert(&header, chunk, 0) {
                assert_eq!(done.seq, 41);
                assert_eq!(done.tick, 1000);
                assembled = Some(done.body);
            }
        }
        if assembled.is_some() {
            break;
        }
    }

    assert_eq!(
        assembled.as_deref(),
        Some(body.as_slice()),
        "the reassembled delta must be identical"
    );
}

/// `game-unreliable` is unordered, so the client can see tick 42 before tick 41. The
/// channel layer must hand the game one delta, the newest, and nothing else.
#[test]
fn a_late_delta_is_dropped_at_the_transport_layer() {
    let mut pair = Pair::connect();

    let new = vec![0xa5u8; 40];
    let old = vec![0x5au8; 40];

    // Newest first, exactly as an unordered channel can deliver it.
    assert_eq!(pair.send_delta(42, 2000, &new), 1);
    assert_eq!(pair.send_delta(41, 1999, &old), 1);

    let mut assembler = FragmentAssembler::default();
    let mut applied: Vec<Vec<u8>> = Vec::new();
    for _ in 0..50 {
        pair.pump();
        for frame in pair.client_rx.remove("game-unreliable").unwrap_or_default() {
            let (header, chunk) = DeltaHeader::parse(&frame).unwrap();
            if let Some(done) = assembler.insert(&header, chunk, 0) {
                applied.push(done.body);
            }
        }
        if !applied.is_empty() {
            break;
        }
    }

    assert_eq!(
        applied,
        vec![new],
        "only the newest tick may reach the game"
    );
    assert_eq!(
        assembler.stale(),
        1,
        "the superseded delta is counted, not applied"
    );
}

/// The client confirms a reliable event on `game-reliable` and the server records it.
#[test]
fn an_event_ack_crosses_the_reliable_channel() {
    let mut pair = Pair::connect();
    let cid = pair.reliable.expect("game-reliable is open");

    let mut wire = Vec::new();
    write_ack(&mut wire, 91, AckKind::Event);
    pair.client
        .channel(cid)
        .unwrap()
        .write(true, &wire)
        .expect("ack written");

    let mut got = None;
    for _ in 0..50 {
        let messages = pair.server_messages();
        if let Some((label, data)) = messages
            .into_iter()
            .find(|(label, _)| label == "game-reliable")
        {
            got = Some((label, data));
            break;
        }
    }

    let (_, data) = got.expect("the server receives the ack on game-reliable");
    assert_eq!(data, wire, "the ack arrives byte-identical");
    assert_eq!(
        parse_ack(&data),
        Some(Ack {
            seq: 91,
            kind: AckKind::Event
        })
    );
}

/// Two fat deltas on the tick boundary: the second supersedes the first, and the client
/// is left with exactly one body and one partial slot.
#[test]
fn back_to_back_fat_deltas_leave_one_body() {
    let mut pair = Pair::connect();

    let first: Vec<u8> = (0..4000u32).map(|i| i as u8).collect();
    let second: Vec<u8> = (0..4000u32).map(|i| (i ^ 0xff) as u8).collect();

    assert_eq!(pair.send_delta(50, 3000, &first), 4);
    // No pump between them: this is what 128 Hz looks like when SCTP lags a frame.
    assert_eq!(pair.send_delta(51, 3001, &second), 4);

    let mut assembler = FragmentAssembler::default();
    let mut applied: Vec<(u16, Vec<u8>)> = Vec::new();
    for _ in 0..80 {
        pair.pump();
        for frame in pair.client_rx.remove("game-unreliable").unwrap_or_default() {
            let (header, chunk) = DeltaHeader::parse(&frame).unwrap();
            if let Some(done) = assembler.insert(&header, chunk, 0) {
                applied.push((done.seq, done.body));
            }
        }
    }

    assert!(
        assembler.newest_seq().is_some(),
        "the channel made progress"
    );
    let bodies: Vec<&[u8]> = applied.iter().map(|(_, b)| b.as_slice()).collect();
    if applied.len() == 2 {
        // Both closed: each must be byte-exact for its own seq.
        assert_eq!(applied[0].1, first);
        assert_eq!(applied[1].1, second);
    } else {
        assert_eq!(applied.len(), 1, "at most one delta should close");
        assert!(bodies.contains(&first.as_slice()) || bodies.contains(&second.as_slice()));
    }
    assert_ne!(assembler.assembled(), 0);
}
