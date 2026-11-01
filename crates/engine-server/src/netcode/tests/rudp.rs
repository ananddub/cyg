use crate::netcode::*;
use crate::plane::reliable::{MAX_RELIABLE_BYTES, ReliableLane};

#[test]
fn reliable_msg_wire_roundtrip_arbitrary_sizes() {
    let mut out = Vec::new();
    for size in [1usize, 9, 64, 512, MAX_RELIABLE_BYTES] {
        let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        encode_reliable_msg(0xBEEF, &payload, &mut out);
        assert_eq!(out[0], CYG_WIRE_RELIABLE_MSG);
        let (seq, got) = decode_reliable_msg(&out).expect("decodes");
        assert_eq!(seq, 0xBEEF);
        assert_eq!(got, payload.as_slice());
    }
    encode_reliable_ack(1247, &mut out);
    assert_eq!(decode_reliable_ack(&out), Some((1247, vec![])));
    assert!(decode_reliable_msg(&[0x05, 1, 0, 9]).is_none(), "wrong op");
    assert!(decode_reliable_ack(&[0x05, 1, 0]).is_none(), "wrong op");
    assert!(decode_reliable_ack(&[0x1F, 1]).is_none(), "short ack");
    assert!(decode_reliable_msg(&[0x1E, 1]).is_none(), "empty payload");
}

#[test]
fn lane_resends_on_loss_and_acks_cumulatively() {
    let mut lane = ReliableLane::new();
    let mut seq = 1u16;
    for i in 0..3 {
        let payload = format!("chat-{i}");
        let mut frame = Vec::new();
        encode_reliable_msg(seq, payload.as_bytes(), &mut frame);
        lane.queue(seq, frame, 0).expect("queues");
        seq += 1;
    }
    let mut sent = 0;
    lane.pump(0, |_| {
        sent += 1;
        true
    });
    assert_eq!(sent, 3);

    let mut attempts = 0;
    lane.pump(5000, |_| {
        attempts += 1;
        attempts == 1
    });
    assert!(attempts >= 1, "resend happened after loss");

    let released = lane.ack(3, 6000);
    assert_eq!(released, 3);
    assert_eq!(lane.inflight(), 0);
}
