use std::collections::HashMap;

use crate::jitter::VoiceJitterEstimator;
use crate::spatial::{DistanceModel, SpatialVoiceRouter};

mod dispatch;

#[cfg(test)]
mod tests;

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

    pub fn join_participant(&mut self, entity_id: u32, team_id: Option<u16>, position: [f64; 3]) {
        let p = VoiceParticipant::new(entity_id, team_id, position);
        self.participants.insert(entity_id, p);
        self.spatial_router.update_participant(entity_id, position);
    }

    pub fn update_position(&mut self, entity_id: u32, position: [f64; 3]) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.position = position;
            self.spatial_router.update_participant(entity_id, position);
        }
    }

    pub fn leave_participant(&mut self, entity_id: u32) {
        self.participants.remove(&entity_id);
        self.spatial_router.remove_participant(entity_id);
    }

    pub fn set_muted(&mut self, entity_id: u32, muted: bool) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.is_muted = muted;
        }
    }

    pub fn set_team(&mut self, entity_id: u32, team_id: Option<u16>) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.team_id = team_id;
        }
    }

    pub fn set_deafened(&mut self, entity_id: u32, deafened: bool) {
        if let Some(p) = self.participants.get_mut(&entity_id) {
            p.is_deafened = deafened;
        }
    }

    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }
}
