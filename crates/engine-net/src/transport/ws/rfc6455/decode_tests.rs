use bytes::BytesMut;

use super::decode::next_ws_frame;

fn build_frame(opcode: u8, fin: bool, mask: Option<[u8; 4]>, payload: &[u8]) -> BytesMut {
    let mut buf = BytesMut::new();
    let b0 = (if fin { 0x80 } else { 0 }) | (opcode & 0x0F);
    buf.extend_from_slice(&[b0]);

    let is_masked = mask.is_some();
    let mask_bit = if is_masked { 0x80 } else { 0 };
    let plen = payload.len();

    if plen <= 125 {
        buf.extend_from_slice(&[mask_bit | (plen as u8)]);
    } else if plen <= 65535 {
        buf.extend_from_slice(&[mask_bit | 126]);
        buf.extend_from_slice(&(plen as u16).to_be_bytes());
    } else {
        buf.extend_from_slice(&[mask_bit | 127]);
        buf.extend_from_slice(&(plen as u64).to_be_bytes());
    }

    if let Some(m) = mask {
        buf.extend_from_slice(&m);
        let mut masked_payload = payload.to_vec();
        for (i, b) in masked_payload.iter_mut().enumerate() {
            *b ^= m[i % 4];
        }
        buf.extend_from_slice(&masked_payload);
    } else {
        buf.extend_from_slice(payload);
    }

    buf
}

#[test]
fn test_decode_incomplete_buffers() {
    let mut empty = BytesMut::new();
    assert!(next_ws_frame(&mut empty).is_none());

    let mut one_byte = BytesMut::from(&[0x82][..]);
    assert!(next_ws_frame(&mut one_byte).is_none());

    let mut partial = build_frame(0x2, true, None, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    partial.truncate(partial.len() - 3);
    assert!(next_ws_frame(&mut partial).is_none());
}

#[test]
fn test_decode_masked_remainder_chunks() {
    let mask = [0xAA, 0xBB, 0xCC, 0xDD];

    for len in [0, 1, 3, 7, 8, 9, 15, 16, 17, 33, 124, 125, 200] {
        let original: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
        let mut buf = build_frame(0x2, true, Some(mask), &original);

        let (op, fin, decoded) = next_ws_frame(&mut buf)
            .expect("frame must decode")
            .expect("not oversize");
        assert_eq!(op, 0x2);
        assert!(fin);
        assert_eq!(&decoded[..], &original[..], "failed for len {}", len);
        assert_eq!(buf.len(), 0, "buffer must be fully consumed");
    }
}

#[test]
fn test_decode_unmasked_frames() {
    let original = vec![42u8; 100];
    let mut buf = build_frame(0x2, true, None, &original);
    let (op, fin, decoded) = next_ws_frame(&mut buf)
        .expect("unmasked frame decode")
        .expect("not oversize");
    assert_eq!(op, 0x2);
    assert!(fin);
    assert_eq!(&decoded[..], &original[..]);
}
