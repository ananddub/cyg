use std::time::Instant;

use bytes::Bytes;

use super::*;
use crate::packet::{VoiceChannelMode, VoicePacket, voice_flags};

#[test]
fn test_voice_room_team_and_mute_filtering() {
    let mut room = VoiceRoom::default();

    room.join_participant(1, Some(1), [0.0, 0.0, 0.0]);

    room.join_participant(2, Some(1), [50.0, 0.0, 0.0]);

    room.join_participant(3, Some(2), [1.0, 0.0, 0.0]);

    let dummy_opus = Bytes::from_static(b"team-radio-encrypted-voice");
    let team_packet = VoicePacket::new(
        1,
        1,
        960,
        VoiceChannelMode::TeamOnly,
        voice_flags::VOICE_ACTIVITY,
        dummy_opus.clone(),
    );

    let frames = room.dispatch_packet(&team_packet, Instant::now());

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].listener_id, 2);

    room.set_muted(1, true);
    let frames_after_mute = room.dispatch_packet(&team_packet, Instant::now());
    assert_eq!(frames_after_mute.len(), 0);
}

#[test]
fn test_set_team_preserves_position_and_mute_state() {
    let mut room = VoiceRoom::default();
    room.join_participant(1, Some(1), [3.0, 4.0, 5.0]);
    room.set_muted(1, true);

    room.set_team(1, Some(7));

    let p = room.participants.get(&1).unwrap();
    assert_eq!(p.team_id, Some(7));
    assert_eq!(p.position, [3.0, 4.0, 5.0]);
    assert!(p.is_muted);

    room.set_team(999, Some(1));
    assert_eq!(room.participant_count(), 1);
}
