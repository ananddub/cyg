use kiddo::MutableKdTree;
use kiddo::dist::SquaredEuclidean;
use std::num::NonZero;

use crate::SpatialQueryResult;

pub struct SpatialIndex3D {
    tree: MutableKdTree<f64, 3>,
}

impl Default for SpatialIndex3D {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialIndex3D {
    pub fn new() -> Self {
        Self {
            tree: MutableKdTree::default(),
        }
    }

    pub fn insert(&mut self, entity_id: u32, pos: [f64; 3]) {
        let _ = self.tree.add(&pos, entity_id);
    }

    pub fn remove(&mut self, entity_id: u32, pos: [f64; 3]) {
        self.tree.remove(&pos, entity_id);
    }

    pub fn update(&mut self, entity_id: u32, old_pos: [f64; 3], new_pos: [f64; 3]) {
        self.remove(entity_id, old_pos);
        self.insert(entity_id, new_pos);
    }

    pub fn query_radius(&self, center: [f64; 3], radius: f64) -> Vec<SpatialQueryResult> {
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

    pub fn for_each_within(&self, center: [f64; 3], radius: f64, mut f: impl FnMut(u32, f64)) {
        if self.tree.size() == 0 {
            return;
        }
        self.tree
            .query(&center)
            .within::<SquaredEuclidean<f64>>(radius * radius)
            .unsorted()
            .visit(|hit| f(hit.item, hit.distance));
    }

    pub fn nearest(&self, query_pos: [f64; 3]) -> Option<SpatialQueryResult> {
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

    pub fn nearest_k(&self, query_pos: [f64; 3], k: usize) -> Vec<SpatialQueryResult> {
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
