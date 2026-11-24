//! `udp_send` keeps every datagram inside the MTU; these check that it actually does.

use super::*;

/// A socket pair on loopback: the server side sends, the test side reads.
fn pair() -> (UdpSocket, UdpSocket) {
    let server = UdpSocket::bind("127.0.0.1:0").expect("bind server");
    let client = UdpSocket::bind("127.0.0.1:0").expect("bind client");
    client
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .expect("read timeout");
    (server, client)
}

fn drain(sock: &UdpSocket, want: usize) -> Vec<Vec<u8>> {
    let mut got = Vec::new();
    let mut buf = vec![0u8; 2048];
    while got.len() < want {
        let Ok(n) = sock.recv(&mut buf) else { break };
        got.push(buf[..n].to_vec());
    }
    got
}

#[test]
fn a_small_frame_stays_one_datagram() {
    let (server, client) = pair();
    let frame = vec![0xA5u8; 400];
    let mut scratch = Vec::new();
    let sent = send(
        &server,
        client.local_addr().unwrap(),
        &frame,
        7,
        99,
        &mut scratch,
    );
    assert_eq!(sent, Some(1));

    let got = drain(&client, 1);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0], frame, "a frame that fits is sent untouched");
}

#[test]
fn an_oversized_frame_is_split_under_the_mtu() {
    let (server, client) = pair();
    // Four chunks' worth: 4 * MAX_DELTA_CHUNK, past any single-datagram budget.
    let frame: Vec<u8> = (0..(MAX_DELTA_CHUNK * 4))
        .map(|i| (i % 251) as u8)
        .collect();
    let mut scratch = Vec::new();
    let sent = send(
        &server,
        client.local_addr().unwrap(),
        &frame,
        11,
        500,
        &mut scratch,
    );
    let total = sent.expect("fragmentable") as usize;
    assert_eq!(total, delta_fragment_count(frame.len()).unwrap() as usize);
    assert!(total >= 4, "a 4-chunk frame needs at least 4 datagrams");

    let got = drain(&client, total);
    assert_eq!(got.len(), total);
    let mut rebuilt = Vec::new();
    for (idx, datagram) in got.iter().enumerate() {
        assert!(
            datagram.len() <= UDP_MTU,
            "datagram {idx} is {} B, over the {UDP_MTU} B MTU",
            datagram.len()
        );
        // Every fragment carries the same seq/tick and its own index, so one frame cannot be
        // mistaken for another and the receiver can order them.
        let header = engine_webrtc_protocol::DeltaHeader::parse(datagram)
            .expect("a fragment parses")
            .0;
        assert_eq!(header.seq, 11);
        assert_eq!(header.tick, 500);
        assert_eq!(header.frag_idx as usize, idx);
        assert_eq!(header.frag_total as usize, total);
        rebuilt.extend_from_slice(&datagram[engine_webrtc_protocol::DELTA_HEADER_LEN..]);
    }
    assert_eq!(rebuilt, frame, "the fragments rebuild the original frame");
}

#[test]
fn a_frame_too_large_to_fragment_is_refused_not_sent_whole() {
    let (server, client) = pair();
    let frame = vec![0u8; engine_webrtc_protocol::MAX_DELTA_BODY + 1];
    let mut scratch = Vec::new();
    let sent = send(
        &server,
        client.local_addr().unwrap(),
        &frame,
        1,
        1,
        &mut scratch,
    );
    assert_eq!(
        sent, None,
        "refused instead of handing the kernel a 300 KB datagram"
    );
    assert!(drain(&client, 1).is_empty(), "nothing reached the wire");
}
