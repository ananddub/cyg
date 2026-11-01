use std::collections::HashSet;

#[test]
fn reordered_frames_all_deliver_and_duplicates_suppress() {
    let mut seen: HashSet<u16> = HashSet::new();
    let mut delivered: Vec<u16> = Vec::new();
    let recv = |seq: u16, seen: &mut HashSet<u16>, delivered: &mut Vec<u16>| {
        if seen.insert(seq) {
            delivered.push(seq);
        }
    };

    recv(3, &mut seen, &mut delivered);
    recv(1, &mut seen, &mut delivered);
    recv(2, &mut seen, &mut delivered);
    assert_eq!(delivered, vec![3, 1, 2], "sab deliver, order-of-arrival me");

    recv(1, &mut seen, &mut delivered);
    assert_eq!(delivered.len(), 3, "duplicate nahi deliver hua");

    recv(5, &mut seen, &mut delivered);
    assert_eq!(delivered.len(), 4);
}
