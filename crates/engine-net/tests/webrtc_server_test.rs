use std::net::SocketAddr;
use std::time::{Duration, Instant};
use engine_net::transport::webrtc::{WebRtcServer, WebRtcServerEvent};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Input, Output, Rtc};

#[test]
fn test_webrtc_server_full_roundtrip() {
    let mut now = Instant::now();
    let mut client_rtc = Rtc::builder().build(now);
    let client_addr: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    let server_addr: SocketAddr = "127.0.0.1:7779".parse().unwrap();

    let client_cand = Candidate::host(client_addr, Protocol::Udp).unwrap();
    let server_cand = Candidate::host(server_addr, Protocol::Udp).unwrap();

    let _ = client_rtc.add_local_candidate(client_cand.clone());

    // 1. Client creates offer with "game-unreliable" and "game-reliable" DataChannels
    let mut change = client_rtc.sdp_api();
    let _cid_unrel = change.add_channel("game-unreliable".into());
    let _cid_rel = change.add_channel("game-reliable".into());
    let (offer, pending) = change.apply().unwrap();
    let offer_sdp = offer.to_sdp_string();

    let mut server = WebRtcServer::new(vec![server_addr]);

    // 2. Server accepts SDP offer and returns SDP answer
    let answer_sdp = server.handle_offer("player_1", &offer_sdp).expect("server handle offer");
    let answer = str0m::change::SdpAnswer::from_sdp_string(&answer_sdp).unwrap();
    client_rtc.sdp_api().accept_answer(pending, answer).unwrap();

    // Add candidates to both peers
    let _ = client_rtc.add_remote_candidate(server_cand);
    let _ = server.handle_candidate("player_1", &client_cand.to_sdp_string());

    let mut client_ch_unrel = None;

    // 3. Exchange UDP packets until DTLS / SCTP Handshake completes
    for _ in 0..100 {
        now += Duration::from_millis(5);

        let _ = client_rtc.handle_input(Input::Timeout(now));
        server.tick(now);

        // Drain client -> server
        while let Ok(out) = client_rtc.poll_output() {
            match out {
                Output::Transmit(t) => {
                    server.handle_udp(client_addr, server_addr, &t.contents);
                }
                Output::Event(str0m::Event::ChannelOpen(cid, label)) => {
                    println!("Client channel open: cid={cid:?}, label={label}");
                    if label == "game-unreliable" {
                        client_ch_unrel = Some(cid);
                    }
                }
                Output::Timeout(_) => break,
                _ => {}
            }
        }

        // Drain server -> client
        for (dest, packet) in server.poll_transmits() {
            if let Ok(recv) = Receive::new(Protocol::Udp, server_addr, dest, &packet) {
                let _ = client_rtc.handle_input(Input::Receive(now, recv));
            }
        }

        if server.is_connected("player_1") && client_ch_unrel.is_some() {
            println!("⚡ Connected and channels opened on both ends!");
            break;
        }
    }

    println!("Server events after handshake:");
    let events = server.poll_events();
    for ev in &events {
        println!(" - {:?}", ev);
    }

    assert!(server.is_connected("player_1"), "player_1 should be marked connected on server");
    assert!(client_ch_unrel.is_some(), "client should have game-unreliable channel opened");

    // 4. Test Client -> Server unreliable input frame transmission
    let test_input = vec![0x01, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut ch = client_rtc.channel(client_ch_unrel.unwrap()).expect("client unreliable channel");
    ch.write(false, &test_input).expect("write to unreliable channel");

    // Flush transmit
    while let Ok(out) = client_rtc.poll_output() {
        match out {
            Output::Transmit(t) => {
                server.handle_udp(client_addr, server_addr, &t.contents);
            }
            Output::Timeout(_) => break,
            _ => {}
        }
    }

    let msgs = server.poll_events();
    let found_msg = msgs.iter().any(|ev| match ev {
        WebRtcServerEvent::Message { client_id, channel_label, data } => {
            println!("Server received msg from {client_id} on '{channel_label}': len={}", data.len());
            client_id == "player_1" && channel_label == "game-unreliable" && data == &test_input
        }
        _ => false,
    });
    assert!(found_msg, "Server should receive input frame on game-unreliable channel");

    // 5. Test Server -> Client unreliable world delta transmission
    let test_delta = vec![0xA5, b'd', b'e', b'l', b't', b'a', 1, 0, 0, 0, 0, 0, 0, 0, 10, 20, 30];
    let ok = server.send_unreliable("player_1", &test_delta);
    assert!(ok, "server send_unreliable should succeed");

    for (dest, packet) in server.poll_transmits() {
        if let Ok(recv) = Receive::new(Protocol::Udp, server_addr, dest, &packet) {
            let _ = client_rtc.handle_input(Input::Receive(now, recv));
        }
    }

    let mut client_got_delta = false;
    while let Ok(out) = client_rtc.poll_output() {
        match out {
            Output::Event(str0m::Event::ChannelData(d)) => {
                if d.data == test_delta {
                    client_got_delta = true;
                    println!("Client received delta: len={}", d.data.len());
                }
            }
            Output::Timeout(_) => break,
            _ => {}
        }
    }
    assert!(client_got_delta, "Client should receive delta frame over WebRTC UDP");
    println!("✓ Full end-to-end WebRTC Client -> [Server] -> Client test PASSED!");
}
