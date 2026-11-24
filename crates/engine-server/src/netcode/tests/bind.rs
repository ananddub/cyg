use crate::netcode::{CYG_WIRE_BIND, encode_bind};

#[test]
fn raw_udp_bind_is_one_frame_for_both_lanes() {
    let mut out = vec![0xAA; 64];
    encode_bind(7, 128.0, "tank-3d", &mut out);
    assert_eq!(out[0], CYG_WIRE_BIND);
    assert_eq!(out[0], 0x24);
    assert_eq!(i64::from_le_bytes(out[1..9].try_into().unwrap()), 7);
    assert_eq!(f64::from_le_bytes(out[9..17].try_into().unwrap()), 128.0);
    assert_eq!(out[17] as usize, "tank-3d".len());
    assert_eq!(&out[18..], b"tank-3d");
    assert_eq!(out.len(), 18 + "tank-3d".len());
}

#[test]
fn a_long_room_name_is_truncated_to_the_u8_length_prefix() {
    let room = "r".repeat(300);
    let mut out = Vec::new();
    encode_bind(0, 30.0, &room, &mut out);
    assert_eq!(out[17], 255);
    assert_eq!(out.len(), 18 + 255);
}
