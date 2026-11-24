use std::collections::HashMap;

use crate::plane::sim::helpers::decode_packed;
use crate::rooms;

#[test]
fn packed_decode_matches_server_convention() {
    let ptr: u32 = 0x0010_0000;
    let len: u32 = 256;
    let packed = ((ptr as i64) << 32) | len as i64;
    assert_eq!(decode_packed(packed), (ptr as usize, len as usize));
}

#[test]
fn packed_decode_handles_high_len() {
    let packed: i64 = ((1u64 << 32) | 0xFFFF_FFFF) as i64;
    assert_eq!(decode_packed(packed), (1, u32::MAX as usize));
}

#[test]
fn newest_wins_updates_seq() {
    let mut last: HashMap<i64, u16> = HashMap::new();
    let mut journal = std::collections::VecDeque::new();
    let (id, seq) = (1i64, 5u16);
    let slot = last.entry(id).or_insert(0);
    assert!(*slot == 0 || engine_webrtc_protocol::is_newer(*slot, seq));
    *slot = seq;
    journal.push_back(rooms::JournalInput {
        tick: 0,
        session: std::rc::Rc::from(""),
        id,
        seq,
        payload: [0u8; 20],
    });
    assert_eq!(last[&id], 5);
    assert_eq!(journal.len(), 1);
}
