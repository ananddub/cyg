use crate::netcode::*;
use crate::plane::reliable::ReliableLane;

fn frame(seq: u16) -> Vec<u8> {
    let mut f = Vec::new();
    encode_reliable_msg(seq, &[seq as u8; 16], &mut f);
    f
}

#[test]
fn sack_releases_delivered_frames_above_the_gap() {
    let mut lane = ReliableLane::new();
    for seq in 1..=3u16 {
        lane.queue(seq, frame(seq), 0).expect("queues");
    }
    lane.pump(0, |_| true);

    let released = lane.ack_selective(0, &[(2, 3)], 100);
    assert_eq!(released, 2, "2 aur 3 release hue");
    assert_eq!(lane.inflight(), 1, "sirf genuinely-missing 1 window me");

    let mut sent_seqs = Vec::new();
    lane.pump(5000, |wire| {
        if let Some((seq, _)) = decode_reliable_msg(wire) {
            sent_seqs.push(seq);
        }
        true
    });
    assert_eq!(sent_seqs, vec![1], "sirf missing frame resend, 2/3 nahi");
}

#[test]
fn sack_wire_roundtrip_and_backcompat() {
    let mut out = Vec::new();
    encode_reliable_ack(9, &mut out);
    assert_eq!(decode_reliable_ack(&out), Some((9, vec![])));
    encode_reliable_ack_sack(1, &[(3, 4), (8, 8)], &mut out);
    assert_eq!(decode_reliable_ack(&out), Some((1, vec![(3, 4), (8, 8)])));
    let mut bad = vec![CYG_WIRE_RELIABLE_ACK, 1, 0, 2, 3, 0, 4, 0];
    assert_eq!(decode_reliable_ack(&bad), Some((1, vec![(3, 4)])));
    bad.truncate(5);
    assert_eq!(decode_reliable_ack(&bad), Some((1, vec![])));
}

#[test]
fn rxd_summary_builds_frontier_and_islands() {
    let mut seen = std::collections::HashSet::new();
    for s in [1u16, 2, 4, 5, 9] {
        seen.insert(s);
    }
    let (cum, ranges) = crate::host::rxd_summary_for_test(&seen);
    assert_eq!(cum, 2);
    assert_eq!(ranges, vec![(4, 5), (9, 9)]);
}
