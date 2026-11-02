use bytes::Bytes;

use super::*;
use crate::packet::{VoiceChannelMode, VoicePacket, voice_flags};

#[test]
fn test_spatial_routing_and_attenuation() {
    let model = DistanceModel::Linear {
        min_distance: 2.0,
        max_distance: 20.0,
    };
    let mut router = SpatialVoiceRouter::new(model);

    let speaker_pos = [0.0, 0.0, 0.0];

    router.update_participant(1, [1.0, 0.0, 0.0]);

    router.update_participant(2, [11.0, 0.0, 0.0]);

    router.update_participant(3, [30.0, 0.0, 0.0]);

    router.update_participant(42, [0.0, 0.0, 0.0]);

    let dummy_opus = Bytes::from_static(b"opus-encoded-voice-data");
    let packet = VoicePacket::new(
        42,
        1,
        960,
        VoiceChannelMode::Proximity,
        voice_flags::VOICE_ACTIVITY,
        dummy_opus,
    );

    let frames = router.route_proximity_packet(&packet, speaker_pos);

    assert_eq!(frames.len(), 2);

    let f1 = frames.iter().find(|f| f.listener_id == 1).unwrap();
    assert!((f1.volume - 1.0).abs() < 0.01);
    assert!((f1.distance - 1.0).abs() < 0.01);

    let f2 = frames.iter().find(|f| f.listener_id == 2).unwrap();
    assert!((f2.volume - 0.5).abs() < 0.01);
    assert!((f2.distance - 11.0).abs() < 0.01);

    assert!(frames.iter().all(|f| f.listener_id != 3));

    assert!(frames.iter().all(|f| f.listener_id != 42));
}
