use bytes::{Bytes, BytesMut};

use super::*;

#[test]
fn test_voice_packet_roundtrip() {
    let raw_opus = Bytes::from_static(b"\x78\x00\x11\x22\x33\x44\x55\x66\x77\x88");
    let packet = VoicePacket::new(
        42,
        105,
        48000,
        VoiceChannelMode::Proximity,
        voice_flags::VOICE_ACTIVITY | voice_flags::STEREO,
        raw_opus.clone(),
    );

    let encoded = packet.encode();
    assert_eq!(encoded.len(), HEADER_LEN + raw_opus.len());

    let decoded = VoicePacket::decode(&encoded[..]).expect("must decode cleanly");
    assert_eq!(decoded.speaker_id, 42);
    assert_eq!(decoded.seq, 105);
    assert_eq!(decoded.timestamp, 48000);
    assert_eq!(decoded.channel_mode, VoiceChannelMode::Proximity);
    assert!(decoded.is_voice_active());
    assert!(decoded.is_stereo());
    assert_eq!(decoded.payload, raw_opus);
}

#[test]
fn test_invalid_magic_rejected() {
    let mut buf = BytesMut::from(&[0x00u8; 20][..]);
    buf[0] = 0x99;
    let err = VoicePacket::decode(&buf.freeze()[..]).unwrap_err();
    assert!(matches!(err, VoicePacketError::InvalidMagic(0x99)));
}
