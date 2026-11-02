use std::collections::{HashMap, HashSet};

use crate::{SpatialIndex2D, VisibilityDelta};

pub struct AoiManager2D {
    index: SpatialIndex2D,
    positions: HashMap<u32, [f64; 2]>,
    visible_sets: HashMap<u32, HashSet<u32>>,

    scratch: HashSet<u32>,
}

impl Default for AoiManager2D {
    fn default() -> Self {
        Self::new()
    }
}

impl AoiManager2D {
    pub fn new() -> Self {
        Self {
            index: SpatialIndex2D::new(),
            positions: HashMap::new(),
            visible_sets: HashMap::new(),
            scratch: HashSet::new(),
        }
    }

    pub fn update_entity(&mut self, entity_id: u32, new_pos: [f64; 2]) {
        if let Some(old_pos) = self.positions.insert(entity_id, new_pos) {
            self.index.update(entity_id, old_pos, new_pos);
        } else {
            self.index.insert(entity_id, new_pos);
        }
    }

    pub fn update_entity_with_delta(
        &mut self,
        entity_id: u32,
        new_pos: [f64; 2],
        aoi_radius: f64,
    ) -> VisibilityDelta {
        self.update_entity(entity_id, new_pos);
        self.compute_visibility_delta(entity_id, aoi_radius)
    }

    pub fn compute_visibility_delta(&mut self, entity_id: u32, aoi_radius: f64) -> VisibilityDelta {
        let Some(&pos) = self.positions.get(&entity_id) else {
            return VisibilityDelta::default();
        };

        self.scratch.clear();
        self.index.for_each_within(pos, aoi_radius, |id, _| {
            if id != entity_id {
                self.scratch.insert(id);
            }
        });

        let prev = self.visible_sets.entry(entity_id).or_default();

        let mut entered: Vec<u32> = Vec::new();
        for &id in &self.scratch {
            if !prev.contains(&id) {
                entered.push(id);
            }
        }
        let mut left: Vec<u32> = Vec::new();
        for &id in &*prev {
            if !self.scratch.contains(&id) {
                left.push(id);
            }
        }

        if entered.is_empty() && left.is_empty() {
            return VisibilityDelta::default();
        }

        for &id in &entered {
            prev.insert(id);
        }
        for &id in &left {
            prev.remove(&id);
        }

        entered.sort_unstable();
        left.sort_unstable();

        VisibilityDelta { entered, left }
    }

    pub fn remove_entity(&mut self, entity_id: u32) -> VisibilityDelta {
        let prev_visible = self.visible_sets.remove(&entity_id).unwrap_or_default();
        if let Some(pos) = self.positions.remove(&entity_id) {
            self.index.remove(entity_id, pos);
        }

        let mut left: Vec<u32> = prev_visible.into_iter().collect();
        left.sort_unstable();

        VisibilityDelta {
            entered: Vec::new(),
            left,
        }
    }

    pub fn get_visible_entities(&self, entity_id: u32, aoi_radius: f64) -> Vec<u32> {
        let Some(&pos) = self.positions.get(&entity_id) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        self.index.for_each_within(pos, aoi_radius, |id, _| {
            if id != entity_id {
                out.push(id);
            }
        });
        out
    }

    pub fn get_observers(&self, entity_id: u32, aoi_radius: f64) -> Vec<u32> {
        self.get_visible_entities(entity_id, aoi_radius)
    }

    pub fn count(&self) -> usize {
        self.positions.len()
    }
}
