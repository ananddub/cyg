use super::*;
use bytes::Bytes;
use std::time::Instant;

#[test]
fn benchmark_100_player_spatial_voice_fanout() {
    let mut room = VoiceRoom::new(DistanceModel::Inverse {
        min_distance: 2.0,
        max_distance: 30.0,
        rolloff: 1.0,
    });

    for id in 0..100 {
        let x = ((id % 10) as f64) * 5.0;
        let y = 0.0;
        let z = ((id / 10) as f64) * 5.0;
        room.join_participant(id, Some((id % 2) as u32), [x, y, z]);
    }

    let dummy_opus = Bytes::from_static(b"opus-20ms-voice-payload-sample-frame-48khz");
    let packet = VoicePacket::new(
        0,
        1,
        960,
        VoiceChannelMode::Proximity,
        voice_flags::VOICE_ACTIVITY,
        dummy_opus,
    );

    let start = Instant::now();
    let iterations = 1_000;
    let mut total_routed = 0;

    for _ in 0..iterations {
        let frames = room.dispatch_packet(&packet, Instant::now());
        total_routed += frames.len();
    }

    let elapsed = start.elapsed();
    let per_packet = elapsed / iterations;

    tracing::info!(
        "Spatial Voice Fanout: 1,000 packets dispatched across 100 players in {:?} ({:?} per packet, total frames routed: {})",
        elapsed,
        per_packet,
        total_routed
    );

    assert!(
        per_packet.as_micros() < 200,
        "Must route in < 200 microseconds"
    );
    assert!(total_routed > 0);
}
