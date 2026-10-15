//! Real in-process WebRTC handshake over the Tokio-plane session engine.
//!
//! Drives a server-side [`RtcSession`] (the plane's per-client session pump:
//! `poll_timeout` → `handle_input` → drain `poll_output` → `Outbound`) against a
//! second str0m `Rtc` acting as the browser, over an in-memory UDP pipe. Proves:
//! - DTLS + SCTP actually complete,
//! - both DataChannels open with the enforced labels and reliability contract,
//! - a 5 KB delta fragments through the codec and reassembles byte-exact,
//! - no wire datagram ever exceeds `MAX_FRAME` (1200).

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{DeltaHeader, FragmentAssembler};
use engine_net::rtc_plane::{LABEL_RELIABLE, LABEL_UNRELIABLE, Outbound, RtcSession};
use str0m::change::SdpOffer;
use str0m::channel::ChannelId;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

fn server_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40_001)
}
fn client_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40_002)
}

/// The "browser": a bare str0m `Rtc` that accepts our offer and answers.
struct Browser {
    rtc: Rtc,
    events: Vec<Event>,
    next: Instant,
}

impl Browser {
    fn new(now: Instant) -> Self {
        let mut rtc = Rtc::new(now);
        rtc.add_local_candidate(Candidate::host(client_addr(), Protocol::Udp).unwrap());
        Self { rtc, events: Vec::new(), next: now }
    }

    fn drain(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            match self.rtc.poll_output().expect("poll") {
                Output::Timeout(t) => {
                    self.next = t;
                    break;
                }
                Output::Transmit(tr) => {
                    assert!(tr.contents.len() <= MAX_FRAME, "wire datagram > MAX_FRAME");
                    out.push(tr.contents.to_vec());
                }
                Output::Event(e) => self.events.push(e),
            }
        }
        out
    }

    fn feed(&mut self, now: Instant, bytes: &[u8]) -> Vec<Vec<u8>> {
        let recv = Receive::new(Protocol::Udp, server_addr(), client_addr(), bytes).unwrap();
        self.rtc.handle_input(Input::Receive(now, recv)).expect("feed");
        self.drain()
    }

    fn tick(&mut self, now: Instant) -> Vec<Vec<u8>> {
        self.rtc.handle_input(Input::Timeout(now)).expect("tick");
        self.drain()
    }

    fn channel_open_label(&self, want: &str) -> Option<ChannelId> {
        self.events.iter().find_map(|e| match e {
            Event::ChannelOpen(id, label) if label == want => Some(*id),
            _ => None,
        })
    }
}

fn datagrams_of(outbound: Vec<Outbound>) -> Vec<Vec<u8>> {
    outbound
        .into_iter()
        .filter_map(|o| match o {
            Outbound::Datagram { contents, .. } => {
                assert!(contents.len() <= MAX_FRAME, "session wire datagram > MAX_FRAME");
                Some(contents)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn real_handshake_two_channels_delta_roundtrip() {
    let now = Instant::now();

    // Server session makes the offer with the enforced channel contract.
    let (mut server, offer) =
        RtcSession::new_server("sess-1".into(), server_addr(), &[server_addr()]).unwrap();

    let mut browser = Browser::new(now);
    let answer = browser
        .rtc
        .sdp_api()
        .accept_offer(SdpOffer::from_sdp_string(&offer).unwrap())
        .expect("accept offer");
    let answer_sdp = answer.to_sdp_string();

    // Prime both sides' first timeouts / transmits.
    let browser_tx = browser.drain();
    let server_tx = datagrams_of(server.accept_answer(&answer_sdp).unwrap());

    let mut to_browser: VecDeque<Vec<u8>> = VecDeque::new();
    let mut to_server: VecDeque<Vec<u8>> = VecDeque::new();
    for b in browser_tx {
        to_server.push_back(b);
    }
    for s in server_tx {
        to_browser.push_back(s);
    }

    let mut now = now;
    let mut connected = false;
    // Bound the pump; a lossless pipe converges in tens of iterations.
    for _ in 0..20_000 {
        while let Some(bytes) = to_browser.pop_front() {
            for t in browser.feed(now, &bytes) {
                to_server.push_back(t);
            }
        }
        while let Some(bytes) = to_server.pop_front() {
            for t in datagrams_of(server.feed_datagram(now, client_addr(), &bytes).unwrap()) {
                to_browser.push_back(t);
            }
        }

        if !connected && server.is_connected() {
            connected = true;
        }
        let (s_unrel, s_rel) = server.channel_ids();
        let both_open = s_unrel.is_some()
            && s_rel.is_some()
            && browser.channel_open_label(LABEL_UNRELIABLE).is_some()
            && browser.channel_open_label(LABEL_RELIABLE).is_some();
        if connected && both_open {
            break;
        }

        let next = server.poll_timeout().min(browser.next);
        now = now.max(next);
        for t in datagrams_of(server.feed_timeout(now)) {
            to_browser.push_back(t);
        }
        for t in browser.tick(now) {
            to_server.push_back(t);
        }
    }

    assert!(connected, "DTLS/SCTP handshake never completed");
    let (unrel, rel) = server.channel_ids();
    assert!(unrel.is_some() && rel.is_some(), "server channels did not open");

    let browser_unrel = browser
        .channel_open_label(LABEL_UNRELIABLE)
        .expect("browser sees game-unreliable open");
    let browser_rel = browser
        .channel_open_label(LABEL_RELIABLE)
        .expect("browser sees game-reliable open");

    // The browser is the *answerer* of channels we declared; verify the reliability
    // contract survived SDP negotiation on the browser's view of the channels.
    let unrel_cfg = browser.rtc.channel(browser_unrel).expect("chan").config().cloned();
    let rel_cfg = browser.rtc.channel(browser_rel).expect("chan").config().cloned();
    assert!(unrel_cfg.as_ref().is_some_and(|c| !c.ordered), "unreliable must be unordered");
    assert!(rel_cfg.as_ref().is_some_and(|c| c.ordered), "reliable must be ordered");

    // ---- 5 KB delta: fragment on server, reassemble on browser, assert byte-exact.
    let body: Vec<u8> = (0..5_000usize).map(|i| (i % 251) as u8).collect();
    let seq: u16 = 0x1234;
    let tick: u32 = 0x00FF_00FF;
    for t in datagrams_of(server.send_delta(seq, tick, &body).unwrap()) {
        to_browser.push_back(t);
    }

    let mut assembler = FragmentAssembler::default();
    let mut assembled = None;
    for _ in 0..20_000 {
        while let Some(bytes) = to_browser.pop_front() {
            for t in browser.feed(now, &bytes) {
                to_server.push_back(t);
            }
        }
        while let Some(bytes) = to_server.pop_front() {
            for t in datagrams_of(server.feed_datagram(now, client_addr(), &bytes).unwrap()) {
                to_browser.push_back(t);
            }
        }
        let drained = std::mem::take(&mut browser.events);
        for ev in drained {
            if let Event::ChannelData(data) = ev {
                let (header, chunk) = DeltaHeader::parse(&data.data).expect("delta frame");
                if let Some(done) = assembler.insert(&header, chunk, 0) {
                    assembled = Some(done);
                }
            }
        }
        if assembled.is_some() {
            break;
        }
        let next = browser.next.min(server.poll_timeout());
        now = now.max(next);
        for t in browser.tick(now) {
            to_server.push_back(t);
        }
        for t in datagrams_of(server.feed_timeout(now)) {
            to_browser.push_back(t);
        }
    }

    let done = assembled.expect("5 KB delta did not reassemble");
    assert_eq!(done.seq, seq);
    assert_eq!(done.tick, tick);
    assert_eq!(done.body, body, "reassembled delta differs from source");
    // The delta left the server as many small frames, never one oversized blob.
    assert!(assembler.assembled() == 1, "exactly one delta should complete");
}
