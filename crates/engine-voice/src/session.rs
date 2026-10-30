//! # Voice Session & Multi-Channel Room Manager
//!
//! Manages active participants, mute/deafen states, team rosters, and coordinates
//! Proximity, Team, Global, and Whisper channels.

use crate::jitter::VoiceJitterEstimator;
use crate::packet::{VoiceChannelMode, VoicePacket};
use crate::spatial::{DistanceModel, RoutedVoiceFrame, SpatialVoiceRouter};
use std::collections::HashMap;
use std::time::Instant;

/// State of an active voice participant.
#[derive(Debug, Clone)]
pub struct VoiceParticipant {
    pub entity_id: u32,
    pub team_id: Option<u16>,
    pub is_muted: bool,
    pub is_deafened: bool,
    pub position: [f64; 3],
    pub jitter: VoiceJitterEstimator,
}

impl VoiceParticipant {
    pub fn new(entity_id: u32, team_id: Option<u16>, position: [f64; 3]) -> Self {
        Self {
            entity_id,
            team_id,
            is_muted: false,
            is_deafened: false,
            position,
            jitter: VoiceJitterEstimator::default(),
        }
    }
}

/// Gaming-grade multi-channel voice room.
pub struct VoiceRoom {
    participants: HashMap<u32, VoiceParticipant>,
    spatial_router: SpatialVoiceRouter,
}

impl Default for VoiceRoom {
    fn default() -> Self {
        Self::new(DistanceModel::default())
    }
}

impl VoiceRoom {
    pub fn new(distance_model: DistanceModel) -> Self {
        Self {
            participants: HashMap::new(),
            spatial_router: SpatialVoiceRouter::new(distance_model),
        }
    }

    /// Add or update a participant in the voice room.
    pub fn join_participant(&mut self, entity_id: u32, team_id: Option<u16>, position: [f64; 3]) {
        let p = VoiceParticipant::new(entity_id, team_id, position);
        self.participants.insert(entity_id, p);
        self.spatial_router.update_participant(entity_id, position);
    }

    /// Update participant's 3D position in the game world.
    pub fn update_position(&mut self, entity_id: u32, position: [f64; 3]) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.position = position;
            self.spatial_router.update_participant(entity_id, position);
        }
    }

    /// Remove a participant on leave.
    pub fn leave_participant(&mut self, entity_id: u32) {
        self.participants.remove(&entity_id);
        self.spatial_router.remove_participant(entity_id);
    }

    /// Set mute status (cannot speak).
    pub fn set_muted(&mut self, entity_id: u32, muted: bool) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.is_muted = muted;
        }
    }

    /// Move a participant to another team, keeping position, mute/deafen state and
    /// jitter history. Re-`join_participant`ing to change a team would wipe all of
    /// that, which is why this exists.
    pub fn set_team(&mut self, entity_id: u32, team_id: Option<u16>) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.team_id = team_id;
        }
    }

    /// Set deafen status (cannot hear anyone).
    pub fn set_deafened(&mut self, entity_id: u32, deafened: bool) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.is_deafened = deafened;
        }
    }

    /// Ingest an inbound voice packet from a player and compute target frames.
    ///
    /// Respects:
    /// - Speaker mute states (returns empty if muted).
    /// - Listener deafen states (deafened listeners receive no packets).
    /// - Channel modes: Proximity (3D KD-Tree), TeamOnly, Global, Whisper.
    pub fn dispatch_packet(&mut self, packet: &VoicePacket, now: Instant) -> Vec<RoutedVoiceFrame> {
        // Speaker verification
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
                // Filter out any listeners that are currently deafened
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
            VoiceChannelMode::Whisper => {
                // Whisper target encoded in speaker_id / metadata or direct unicast
                Vec::new()
            }
        }
    }

    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::voice_flags;
    use bytes::Bytes;

    #[test]
    fn test_voice_room_team_and_mute_filtering() {
        let mut room = VoiceRoom::default();

        // Player 1: Team 1
        room.join_participant(1, Some(1), [0.0, 0.0, 0.0]);
        // Player 2: Team 1 (Teammate)
        room.join_participant(2, Some(1), [50.0, 0.0, 0.0]); // far away
        // Player 3: Team 2 (Enemy)
        room.join_participant(3, Some(2), [1.0, 0.0, 0.0]); // close, but enemy

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

        // Only Player 2 (teammate) should receive the frame, even if 50m away!
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].listener_id, 2);

        // Now mute Player 1
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

        // Re-joining to change a team reset position, mute and jitter history;
        // set_team must leave all of it intact.
        let p = room.participants.get(&1).unwrap();
        assert_eq!(p.team_id, Some(7));
        assert_eq!(p.position, [3.0, 4.0, 5.0]);
        assert!(p.is_muted);

        // Unknown participants are a no-op, not an accidental spawn.
        room.set_team(999, Some(1));
        assert_eq!(room.participant_count(), 1);
    }
}
