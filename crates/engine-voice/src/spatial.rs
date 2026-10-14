//! # 3D Proximity Spatial Audio Routing
//!
//! Powered by `engine-spatial` (SIMD KD-Trees).
//! Automatically computes:
//! - Distance attenuation curve (Linear or Inverse Distance).
//! - Hearing radius culling (0 bandwidth outside max range).
//! - Relative 3D direction vectors for binaural / stereo panning in clients.

use std::collections::HashMap;
use bytes::Bytes;
use engine_spatial::SpatialIndex3D;
use crate::packet::VoicePacket;

/// Distance attenuation curve for 3D positional voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DistanceModel {
    /// Linear attenuation from min_distance (1.0 volume) to max_distance (0.0 volume).
    Linear {
        min_distance: f64,
        max_distance: f64,
    },
    /// Physically realistic inverse distance model:
    /// V = min_distance / (min_distance + rolloff * (distance - min_distance))
    Inverse {
        min_distance: f64,
        max_distance: f64,
        rolloff: f64,
    },
}

impl Default for DistanceModel {
    fn default() -> Self {
        Self::Inverse {
            min_distance: 1.5,
            max_distance: 25.0,
            rolloff: 1.0,
        }
    }
}

impl DistanceModel {
    pub fn max_distance(&self) -> f64 {
        match self {
            Self::Linear { max_distance, .. } => *max_distance,
            Self::Inverse { max_distance, .. } => *max_distance,
        }
    }

    /// Compute volume multiplier in range [0.0, 1.0].
    pub fn compute_volume(&self, distance: f64) -> f32 {
        match *self {
            Self::Linear { min_distance, max_distance } => {
                if distance <= min_distance {
                    1.0
                } else if distance >= max_distance {
                    0.0
                } else {
                    let factor = (max_distance - distance) / (max_distance - min_distance);
                    factor.clamp(0.0, 1.0) as f32
                }
            }
            Self::Inverse { min_distance, max_distance, rolloff } => {
                if distance <= min_distance {
                    1.0
                } else if distance >= max_distance {
                    0.0
                } else {
                    let denom = min_distance + rolloff * (distance - min_distance);
                    if denom <= 0.0 {
                        1.0
                    } else {
                        ((min_distance / denom) as f32).clamp(0.0, 1.0)
                    }
                }
            }
        }
    }
}

/// A routed voice frame targeted to an individual listener.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutedVoiceFrame {
    /// Target listener entity ID.
    pub listener_id: u32,
    /// Originating speaker entity ID.
    pub speaker_id: u32,
    /// Monotonic sequence number.
    pub seq: u16,
    /// RTP timestamp.
    pub timestamp: u32,
    /// Distance-attenuated volume multiplier [0.0, 1.0].
    pub volume: f32,
    /// Normalized 3D direction vector from listener to speaker: [dx, dy, dz].
    /// Used by client Web Audio PannerNode or Unity 3D Audio Source.
    pub relative_direction: [f32; 3],
    /// Exact distance in game world units.
    pub distance: f32,
    /// Zero-copy Opus payload refcount bump.
    pub payload: Bytes,
}

/// Sub-microsecond 3D Spatial Audio Router powered by SIMD KD-Trees.
pub struct SpatialVoiceRouter {
    /// 3D spatial index for fast radius searches.
    spatial_index: SpatialIndex3D,
    /// Listener positions map for fast vector computation.
    positions: HashMap<u32, [f64; 3]>,
    /// Active distance attenuation model.
    distance_model: DistanceModel,
}

impl Default for SpatialVoiceRouter {
    fn default() -> Self {
        Self::new(DistanceModel::default())
    }
}

impl SpatialVoiceRouter {
    pub fn new(distance_model: DistanceModel) -> Self {
        Self {
            spatial_index: SpatialIndex3D::new(),
            positions: HashMap::new(),
            distance_model,
        }
    }

    /// Set or update a participant's 3D position in the world.
    pub fn update_participant(&mut self, entity_id: u32, pos: [f64; 3]) {
        if let Some(old_pos) = self.positions.insert(entity_id, pos) {
            self.spatial_index.update(entity_id, old_pos, pos);
        } else {
            self.spatial_index.insert(entity_id, pos);
        }
    }

    /// Remove a participant when they disconnect or leave the room.
    pub fn remove_participant(&mut self, entity_id: u32) {
        if let Some(pos) = self.positions.remove(&entity_id) {
            self.spatial_index.remove(entity_id, pos);
        }
    }

    /// Route a voice packet to all listeners within audible hearing range.
    ///
    /// Automatically applies:
    /// - KD-Tree hearing radius culling (listeners out of range are omitted).
    /// - Distance volume decay based on distance model.
    /// - Stereo/binaural direction vectors.
    pub fn route_proximity_packet(
        &self,
        packet: &VoicePacket,
        speaker_pos: [f64; 3],
    ) -> Vec<RoutedVoiceFrame> {
        let max_range = self.distance_model.max_distance();
        let hits = self.spatial_index.query_radius(speaker_pos, max_range);

        let mut routed = Vec::with_capacity(hits.len());

        for hit in hits {
            let listener_id = hit.entity_id;
            // Never echo back to the speaker themselves
            if listener_id == packet.speaker_id {
                continue;
            }

            let distance = hit.distance;
            let volume = self.distance_model.compute_volume(distance);
            if volume <= 0.001 {
                continue;
            }

            // Compute relative direction from listener to speaker
            let rel_dir = if let Some(listener_pos) = self.positions.get(&listener_id) {
                let dx = speaker_pos[0] - listener_pos[0];
                let dy = speaker_pos[1] - listener_pos[1];
                let dz = speaker_pos[2] - listener_pos[2];
                let len = (dx * dx + dy * dy + dz * dz).sqrt();
                if len > 0.0001 {
                    [(dx / len) as f32, (dy / len) as f32, (dz / len) as f32]
                } else {
                    [0.0, 0.0, 1.0]
                }
            } else {
                [0.0, 0.0, 1.0]
            };

            routed.push(RoutedVoiceFrame {
                listener_id,
                speaker_id: packet.speaker_id,
                seq: packet.seq,
                timestamp: packet.timestamp,
                volume,
                relative_direction: rel_dir,
                distance: distance as f32,
                payload: packet.payload.clone(), // Zero-copy Bytes ref bump
            });
        }

        routed
    }

    /// Active participant count.
    pub fn participant_count(&self) -> usize {
        self.positions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{voice_flags, VoiceChannelMode};

    #[test]
    fn test_spatial_routing_and_attenuation() {
        let model = DistanceModel::Linear {
            min_distance: 2.0,
            max_distance: 20.0,
        };
        let mut router = SpatialVoiceRouter::new(model);

        // Speaker at [0.0, 0.0, 0.0]
        let speaker_pos = [0.0, 0.0, 0.0];

        // Listener 1: Close (1.0m away) -> 100% volume
        router.update_participant(1, [1.0, 0.0, 0.0]);
        // Listener 2: Mid-range (11.0m away) -> 50% volume
        router.update_participant(2, [11.0, 0.0, 0.0]);
        // Listener 3: Far out (30.0m away) -> Culled out of range!
        router.update_participant(3, [30.0, 0.0, 0.0]);
        // Speaker themselves (should not receive echo)
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

        // Only Listener 1 & Listener 2 should receive frames
        assert_eq!(frames.len(), 2);

        let f1 = frames.iter().find(|f| f.listener_id == 1).unwrap();
        assert!((f1.volume - 1.0).abs() < 0.01);
        assert!((f1.distance - 1.0).abs() < 0.01);

        let f2 = frames.iter().find(|f| f.listener_id == 2).unwrap();
        assert!((f2.volume - 0.5).abs() < 0.01);
        assert!((f2.distance - 11.0).abs() < 0.01);

        // Listener 3 (30m) must be culled (not present)
        assert!(frames.iter().all(|f| f.listener_id != 3));
        // Speaker 42 must not receive self-echo
        assert!(frames.iter().all(|f| f.listener_id != 42));
    }
}
