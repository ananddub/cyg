use bytes::Bytes;
use engine_spatial::SpatialIndex3D;
use std::collections::HashMap;

use super::DistanceModel;
use crate::packet::VoicePacket;

#[derive(Debug, Clone, PartialEq)]
pub struct RoutedVoiceFrame {
    pub listener_id: u32,
    pub speaker_id: u32,
    pub seq: u16,
    pub timestamp: u32,
    pub volume: f32,
    pub relative_direction: [f32; 3],
    pub distance: f32,
    pub payload: Bytes,
}

pub struct SpatialVoiceRouter {
    spatial_index: SpatialIndex3D,
    positions: HashMap<u32, [f64; 3]>,
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

    pub fn update_participant(&mut self, entity_id: u32, pos: [f64; 3]) {
        if let Some(old_pos) = self.positions.insert(entity_id, pos) {
            self.spatial_index.update(entity_id, old_pos, pos);
        } else {
            self.spatial_index.insert(entity_id, pos);
        }
    }

    pub fn remove_participant(&mut self, entity_id: u32) {
        if let Some(pos) = self.positions.remove(&entity_id) {
            self.spatial_index.remove(entity_id, pos);
        }
    }

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

            if listener_id == packet.speaker_id {
                continue;
            }

            let distance = hit.distance;
            let volume = self.distance_model.compute_volume(distance);
            if volume <= 0.001 {
                continue;
            }

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
                payload: packet.payload.clone(),
            });
        }

        routed
    }

    pub fn participant_count(&self) -> usize {
        self.positions.len()
    }
}
