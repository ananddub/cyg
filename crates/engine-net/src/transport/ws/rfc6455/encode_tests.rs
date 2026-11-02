use bytes::BytesMut;

use super::encode::encode_wire_frame;

#[test]
fn test_encode_wire_frame_boundaries() {

    let mut out = BytesMut::new();
    encode_wire_frame(15, &[], &mut out);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0], 0x82);
    assert_eq!(out[1], 1);
    assert_eq!(out[2], 15);

    out.clear();
    let p124 = vec![0xAA; 124];
    encode_wire_frame(15, &p124, &mut out);
    assert_eq!(out.len(), 124 + 3);
    assert_eq!(out[0], 0x82);
    assert_eq!(out[1], 125);
    assert_eq!(out[2], 15);
    assert_eq!(&out[3..], &p124[..]);

    out.clear();
    let p125 = vec![0xBB; 125];
    encode_wire_frame(15, &p125, &mut out);
    assert_eq!(out.len(), 125 + 5);
    assert_eq!(out[0], 0x82);
    assert_eq!(out[1], 126);
    assert_eq!(&out[2..4], &126u16.to_be_bytes());
    assert_eq!(out[4], 15);
    assert_eq!(&out[5..], &p125[..]);

    out.clear();
    let p65534 = vec![0xCC; 65534];
    encode_wire_frame(15, &p65534, &mut out);
    assert_eq!(out.len(), 65534 + 5);
    assert_eq!(out[0], 0x82);
    assert_eq!(out[1], 126);
    assert_eq!(&out[2..4], &65535u16.to_be_bytes());
    assert_eq!(out[4], 15);

    out.clear();
    let p65535 = vec![0xDD; 65535];
    encode_wire_frame(15, &p65535, &mut out);
    assert_eq!(out.len(), 65535 + 11);
    assert_eq!(out[0], 0x82);
    assert_eq!(out[1], 127);
    assert_eq!(&out[2..10], &65536u64.to_be_bytes());
    assert_eq!(out[10], 15);
}
