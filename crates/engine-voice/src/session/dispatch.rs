use std::time::Instant;

use super::VoiceRoom;
use crate::packet::{VoiceChannelMode, VoicePacket};
use crate::spatial::RoutedVoiceFrame;

impl VoiceRoom {
    pub fn dispatch_packet(&mut self, packet: &VoicePacket, now: Instant) -> Vec<RoutedVoiceFrame> {
        let (speaker_muted, speaker_pos, speaker_team) =
            match self.participants.get_mut(&packet.speaker_id) {
                Some(p) => {
                    if p.is_muted {
                        return Vec::new();
                    }
                    p.jitter.record_packet(packet.seq, packet.timestamp, now);
                    (p.is_muted, p.position, p.team_id)
                }
                None => return Vec::new(),
            };

        if speaker_muted {
            return Vec::new();
        }

        match packet.channel_mode {
            VoiceChannelMode::Proximity => {
                let frames = self
                    .spatial_router
                    .route_proximity_packet(packet, speaker_pos);

                frames
                    .into_iter()
                    .filter(|f| {
                        self.participants
                            .get(&f.listener_id)
                            .map(|p| !p.is_deafened)
                            .unwrap_or(false)
                    })
                    .collect()
            }
            VoiceChannelMode::Global => {
                let mut out = Vec::new();
                for (&id, p) in &self.participants {
                    if id != packet.speaker_id && !p.is_deafened {
                        out.push(RoutedVoiceFrame {
                            listener_id: id,
                            speaker_id: packet.speaker_id,
                            seq: packet.seq,
                            timestamp: packet.timestamp,
                            volume: 1.0,
                            relative_direction: [0.0, 0.0, 0.0],
                            distance: 0.0,
                            payload: packet.payload.clone(),
                        });
                    }
                }
                out
            }
            VoiceChannelMode::TeamOnly => {
                let mut out = Vec::new();
                if let Some(speaker_t) = speaker_team {
                    for (&id, p) in &self.participants {
                        if id != packet.speaker_id && !p.is_deafened && p.team_id == Some(speaker_t)
                        {
                            out.push(RoutedVoiceFrame {
                                listener_id: id,
                                speaker_id: packet.speaker_id,
                                seq: packet.seq,
                                timestamp: packet.timestamp,
                                volume: 1.0,
                                relative_direction: [0.0, 0.0, 0.0],
                                distance: 0.0,
                                payload: packet.payload.clone(),
                            });
                        }
                    }
                }
                out
            }
            VoiceChannelMode::Whisper => Vec::new(),
        }
    }
}
