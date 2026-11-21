use kiddo::MutableKdTree;
use kiddo::dist::SquaredEuclidean;
use std::num::NonZero;

use crate::SpatialQueryResult;

pub struct SpatialIndex2D {
    tree: MutableKdTree<f64, 2>,

    last_pos: std::collections::HashMap<u32, [f64; 2]>,
}

impl Default for SpatialIndex2D {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialIndex2D {
    pub fn new() -> Self {
        Self {
            tree: MutableKdTree::default(),
            last_pos: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, entity_id: u32, pos: [f64; 2]) {
        let _ = self.tree.add(&pos, entity_id);
    }

    pub fn remove(&mut self, entity_id: u32, pos: [f64; 2]) {
        // Prefer the tracked position: a guest-supplied coordinate may drift
        // from where the entity actually lives, and forgetting to pop
        // `last_pos` would leave a stale entry behind the next upsert.
        if let Some(last) = self.last_pos.remove(&entity_id) {
            self.tree.remove(&last, entity_id);
        } else {
            self.tree.remove(&pos, entity_id);
        }
    }

    pub fn upsert(&mut self, entity_id: u32, pos: [f64; 2]) {
        if let Some(old) = self.last_pos.insert(entity_id, pos) {
            self.tree.remove(&old, entity_id);
        }
        let _ = self.tree.add(&pos, entity_id);
    }

    pub fn update(&mut self, entity_id: u32, old_pos: [f64; 2], new_pos: [f64; 2]) {
        self.remove(entity_id, old_pos);
        self.insert(entity_id, new_pos);
    }

    pub fn query_radius(&self, center: [f64; 2], radius: f64) -> Vec<SpatialQueryResult> {
        let mut out = Vec::new();
        self.for_each_within(center, radius, |entity_id, distance_squared| {
            out.push(SpatialQueryResult {
                entity_id,
                distance_squared,
                distance: distance_squared.sqrt(),
            });
        });
        out
    }

    pub fn for_each_within(&self, center: [f64; 2], radius: f64, mut f: impl FnMut(u32, f64)) {
        if self.tree.size() == 0 {
            return;
        }
        self.tree
            .query(&center)
            .within::<SquaredEuclidean<f64>>(radius * radius)
            .unsorted()
            .visit(|hit| f(hit.item, hit.distance));
    }

    pub fn nearest(&self, query_pos: [f64; 2]) -> Option<SpatialQueryResult> {
        if self.tree.size() == 0 {
            return None;
        }
        let hit = self
            .tree
            .query(&query_pos)
            .nearest_one::<SquaredEuclidean<f64>>()
            .execute();

        Some(SpatialQueryResult {
            entity_id: hit.item,
            distance_squared: hit.distance,
            distance: hit.distance.sqrt(),
        })
    }

    pub fn nearest_k(&self, query_pos: [f64; 2], k: usize) -> Vec<SpatialQueryResult> {
        let Some(k_non_zero) = NonZero::new(k) else {
            return Vec::new();
        };
        if self.tree.size() == 0 {
            return Vec::new();
        }
        let hits = self
            .tree
            .query(&query_pos)
            .nearest_n::<SquaredEuclidean<f64>>(k_non_zero)
            .execute();

        hits.into_iter()
            .map(|hit| SpatialQueryResult {
                entity_id: hit.item,
                distance_squared: hit.distance,
                distance: hit.distance.sqrt(),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }
}
