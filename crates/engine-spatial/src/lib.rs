//! # Engine Spatial — SIMD-Accelerated 2D & 3D Spatial Partitioning & AOI
//!
//! Powered by `kiddo` v6 KD-Trees. Provides:
//! - Sub-microsecond proximity and nearest-neighbor spatial queries
//! - 2D & 3D Spatial Indexes (`SpatialIndex2D`, `SpatialIndex3D`)
//! - Multiplayer Network Area of Interest Managers (`AoiManager2D`, `AoiManager3D`)
//!   for visibility culling and packet bandwidth reduction.
//! - Delta-buffer interest management ([`aoi`]): guest TLV parse + per-viewer
//!   near/far culling.

pub mod aoi;

pub use aoi::{DeltaEntity, filter_for_viewer, parse_delta, viewer_pos_of};

use std::collections::{HashMap, HashSet};
use std::num::NonZero;
use kiddo::MutableKdTree;
use kiddo::dist::SquaredEuclidean;

/// Result of a spatial radius or nearest-neighbor query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialQueryResult {
    pub entity_id: u32,
    pub distance_squared: f64,
    pub distance: f64,
}

/// Entity identifiers that entered or left visibility after movement.
///
/// Designed for high-frequency multiplayer netcode replication:
/// - `entered`: Newly visible entities -> Send `SpawnEntity` packets to client.
/// - `left`: Entities out of range -> Send `DespawnEntity` packets to client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisibilityDelta {
    /// Entities that became visible during this update.
    pub entered: Vec<u32>,
    /// Entities that left visibility during this update.
    pub left: Vec<u32>,
}

// =========================================================================
// 2D Spatial Index (Top-Down / Platformers / 2D Maps)
// =========================================================================

/// High-performance 2D KD-Tree spatial index for entity proximity & AOI queries.
pub struct SpatialIndex2D {
    tree: MutableKdTree<f64, 2>,
    /// Last known position per entity — lets `upsert` remove the stale leaf
    /// (kiddo removal needs the exact stored coordinates).
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

    /// Insert an entity with 2D coordinate `[x, y]`.
    pub fn insert(&mut self, entity_id: u32, pos: [f64; 2]) {
        let _ = self.tree.add(&pos, entity_id);
    }

    /// Remove an entity from the 2D tree.
    pub fn remove(&mut self, entity_id: u32, pos: [f64; 2]) {
        self.tree.remove(&pos, entity_id);
    }

    /// Update an entity's 2D position.
    /// Insert-or-move in one call — the guest-facing `sys_spatial_update`
    /// path, where the caller does not track whether the entity is new.
    /// Re-calling with a moved entity replaces its leaf (no duplicates).
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

    /// Query all entities within a Euclidean radius `r` of `center`.
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

    /// Streams every entity within radius `r` of `center` to `f` as
    /// `(entity_id, distance_squared)`, allocating nothing. Use this in
    /// per-tick loops instead of [`query_radius`](Self::query_radius).
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

    /// Find the single nearest entity to `query_pos`.
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

    /// Find the k-nearest entities to `query_pos`.
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

    /// Total number of indexed items.
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }
}

// =========================================================================
// 3D Spatial Index (FPS / TPS / CYG Arena / 3D Battlefield)
// =========================================================================

/// High-performance 3D KD-Tree spatial index for entity proximity & AOI queries.
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

    /// Insert an entity with 3D coordinate `[x, y, z]`.
    pub fn insert(&mut self, entity_id: u32, pos: [f64; 3]) {
        let _ = self.tree.add(&pos, entity_id);
    }

    /// Remove an entity from the 3D tree.
    pub fn remove(&mut self, entity_id: u32, pos: [f64; 3]) {
        self.tree.remove(&pos, entity_id);
    }

    /// Update an entity's 3D position.
    pub fn update(&mut self, entity_id: u32, old_pos: [f64; 3], new_pos: [f64; 3]) {
        self.remove(entity_id, old_pos);
        self.insert(entity_id, new_pos);
    }

    /// Query all entities within a 3D sphere radius `r` of `center`.
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

    /// Streams every entity within sphere radius `r` of `center` to `f` as
    /// `(entity_id, distance_squared)`, allocating nothing. Use this in
    /// per-tick loops instead of [`query_radius`](Self::query_radius).
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

    /// Find the single nearest entity to `query_pos`.
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

    /// Find the k-nearest entities to `query_pos`.
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

    /// Total number of indexed items.
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }
}

// =========================================================================
// Area of Interest (AOI) Managers for Network Replication & Visibility
// =========================================================================

/// Authoritative 3D Area of Interest (AOI) Manager for multiplayer network culling.
///
/// Keeps entity positions tracked and resolves interest lists and movement deltas:
/// - Determines which clients should receive updates about which entities based on spatial radius.
/// - Calculates `VisibilityDelta` (`entered` and `left`) on every movement for netcode spawn/despawn triggers.
pub struct AoiManager3D {
    index: SpatialIndex3D,
    positions: HashMap<u32, [f64; 3]>,
    visible_sets: HashMap<u32, HashSet<u32>>,
    /// Query results for the delta currently being computed. Reused between
    /// calls so a warm manager stops rebuilding a hash set every tick.
    scratch: HashSet<u32>,
}

impl Default for AoiManager3D {
    fn default() -> Self {
        Self::new()
    }
}

impl AoiManager3D {
    pub fn new() -> Self {
        Self {
            index: SpatialIndex3D::new(),
            positions: HashMap::new(),
            visible_sets: HashMap::new(),
            scratch: HashSet::new(),
        }
    }

    /// Update or insert an entity's 3D position.
    pub fn update_entity(&mut self, entity_id: u32, new_pos: [f64; 3]) {
        if let Some(old_pos) = self.positions.insert(entity_id, new_pos) {
            self.index.update(entity_id, old_pos, new_pos);
        } else {
            self.index.insert(entity_id, new_pos);
        }
    }

    /// Update an entity's 3D position and compute the visibility delta (`entered` and `left`).
    pub fn update_entity_with_delta(
        &mut self,
        entity_id: u32,
        new_pos: [f64; 3],
        aoi_radius: f64,
    ) -> VisibilityDelta {
        self.update_entity(entity_id, new_pos);
        self.compute_visibility_delta(entity_id, aoi_radius)
    }

    /// Compute the VisibilityDelta for `entity_id` against its previous visible set.
    pub fn compute_visibility_delta(&mut self, entity_id: u32, aoi_radius: f64) -> VisibilityDelta {
        let Some(&pos) = self.positions.get(&entity_id) else {
            return VisibilityDelta::default();
        };

        // `clear` keeps the buckets allocated, so streaming the query into the
        // scratch set costs no hash-table setup per tick.
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

        // The common case: nobody crossed the AOI boundary this tick. Report no
        // change, leave the stored set alone, allocate nothing.
        if entered.is_empty() && left.is_empty() {
            return VisibilityDelta::default();
        }

        for &id in &entered {
            prev.insert(id);
        }
        for &id in &left {
            prev.remove(&id);
        }
        // Sorted so spawn/despawn packet order stays deterministic.
        entered.sort_unstable();
        left.sort_unstable();

        VisibilityDelta { entered, left }
    }

    /// Remove an entity from tracking and return its departure delta.
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

    /// Get all other entities visible to `entity_id` within `aoi_radius`.
    pub fn get_visible_entities(&self, entity_id: u32, aoi_radius: f64) -> Vec<u32> {
        let Some(&pos) = self.positions.get(&entity_id) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        self.index
            .for_each_within(pos, aoi_radius, |id, _| {
                if id != entity_id {
                    out.push(id);
                }
            });
        out
    }

    /// Get all observers currently watching `entity_id` within `aoi_radius`.
    pub fn get_observers(&self, entity_id: u32, aoi_radius: f64) -> Vec<u32> {
        self.get_visible_entities(entity_id, aoi_radius)
    }

    /// Get the total count of active entities tracked.
    pub fn count(&self) -> usize {
        self.positions.len()
    }
}

/// Authoritative 2D Area of Interest (AOI) Manager for 2D multiplayer games.
pub struct AoiManager2D {
    index: SpatialIndex2D,
    positions: HashMap<u32, [f64; 2]>,
    visible_sets: HashMap<u32, HashSet<u32>>,
    /// Query results for the delta currently being computed. Reused between
    /// calls so a warm manager stops rebuilding a hash set every tick.
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

    /// Update or insert an entity's 2D position.
    pub fn update_entity(&mut self, entity_id: u32, new_pos: [f64; 2]) {
        if let Some(old_pos) = self.positions.insert(entity_id, new_pos) {
            self.index.update(entity_id, old_pos, new_pos);
        } else {
            self.index.insert(entity_id, new_pos);
        }
    }

    /// Update an entity's 2D position and compute the visibility delta (`entered` and `left`).
    pub fn update_entity_with_delta(
        &mut self,
        entity_id: u32,
        new_pos: [f64; 2],
        aoi_radius: f64,
    ) -> VisibilityDelta {
        self.update_entity(entity_id, new_pos);
        self.compute_visibility_delta(entity_id, aoi_radius)
    }

    /// Compute the VisibilityDelta for `entity_id` against its previous visible set.
    pub fn compute_visibility_delta(&mut self, entity_id: u32, aoi_radius: f64) -> VisibilityDelta {
        let Some(&pos) = self.positions.get(&entity_id) else {
            return VisibilityDelta::default();
        };

        // `clear` keeps the buckets allocated, so streaming the query into the
        // scratch set costs no hash-table setup per tick.
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

        // The common case: nobody crossed the AOI boundary this tick. Report no
        // change, leave the stored set alone, allocate nothing.
        if entered.is_empty() && left.is_empty() {
            return VisibilityDelta::default();
        }

        for &id in &entered {
            prev.insert(id);
        }
        for &id in &left {
            prev.remove(&id);
        }
        // Sorted so spawn/despawn packet order stays deterministic.
        entered.sort_unstable();
        left.sort_unstable();

        VisibilityDelta { entered, left }
    }

    /// Remove an entity from tracking and return its departure delta.
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

    /// Get all other entities visible to `entity_id` within `aoi_radius`.
    pub fn get_visible_entities(&self, entity_id: u32, aoi_radius: f64) -> Vec<u32> {
        let Some(&pos) = self.positions.get(&entity_id) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        self.index
            .for_each_within(pos, aoi_radius, |id, _| {
                if id != entity_id {
                    out.push(id);
                }
            });
        out
    }

    /// Get all observers currently watching `entity_id` within `aoi_radius`.
    pub fn get_observers(&self, entity_id: u32, aoi_radius: f64) -> Vec<u32> {
        self.get_visible_entities(entity_id, aoi_radius)
    }

    /// Get the total count of active entities tracked.
    pub fn count(&self) -> usize {
        self.positions.len()
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spatial_index_2d_radius_and_nearest() {
        let mut idx = SpatialIndex2D::new();
        idx.insert(1, [0.0, 0.0]);
        idx.insert(2, [10.0, 0.0]);
        idx.insert(3, [50.0, 50.0]);

        // Radius query from origin within 15 meters should find entity 1 and 2
        let hits = idx.query_radius([0.0, 0.0], 15.0);
        let ids: Vec<u32> = hits.iter().map(|h| h.entity_id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(!ids.contains(&3));

        // Nearest to [8.0, 1.0] should be entity 2
        let nearest = idx.nearest([8.0, 1.0]).unwrap();
        assert_eq!(nearest.entity_id, 2);
    }

    #[test]
    fn test_spatial_index_3d_radius_and_aoi() {
        let mut idx = SpatialIndex3D::new();
        idx.insert(101, [0.0, 1.8, 0.0]);
        idx.insert(102, [20.0, 1.8, 0.0]);
        idx.insert(103, [100.0, 1.8, 100.0]);

        // 30m radius around entity 101
        let nearby = idx.query_radius([0.0, 1.8, 0.0], 30.0);
        assert_eq!(nearby.len(), 2);
        let ids: Vec<u32> = nearby.iter().map(|h| h.entity_id).collect();
        assert!(ids.contains(&101));
        assert!(ids.contains(&102));
        assert!(!ids.contains(&103));
    }

    #[test]
    fn test_aoi_manager_3d_visibility_culling() {
        let mut aoi = AoiManager3D::new();
        aoi.update_entity(1, [0.0, 0.0, 0.0]);
        aoi.update_entity(2, [35.0, 0.0, 0.0]);
        aoi.update_entity(3, [250.0, 0.0, 0.0]);

        // Entity 1 with 50m AOI radius: sees entity 2 (35m), does NOT see entity 3 (250m)
        let visible_to_1 = aoi.get_visible_entities(1, 50.0);
        assert_eq!(visible_to_1, vec![2]);

        // Entity 3 with 50m AOI radius: sees nobody
        let visible_to_3 = aoi.get_visible_entities(3, 50.0);
        assert!(visible_to_3.is_empty());

        // Update entity 2 moving far away (300m)
        aoi.update_entity(2, [300.0, 0.0, 0.0]);
        let visible_to_1_after_move = aoi.get_visible_entities(1, 50.0);
        assert!(visible_to_1_after_move.is_empty());
    }

    #[test]
    fn test_aoi_visibility_delta_entered_and_left_3d() {
        let mut aoi = AoiManager3D::new();

        // 1. Entity 1 appears at origin
        let d1 = aoi.update_entity_with_delta(1, [0.0, 0.0, 0.0], 50.0);
        assert!(d1.entered.is_empty());
        assert!(d1.left.is_empty());

        // 2. Entity 2 spawns at (25m, 0, 0) inside entity 1's AOI radius (50m)
        aoi.update_entity(2, [25.0, 0.0, 0.0]);
        let d2 = aoi.compute_visibility_delta(1, 50.0);
        assert_eq!(d2.entered, vec![2]);
        assert!(d2.left.is_empty());

        // 3. Entity 2 moves far away to (200m, 0, 0) outside AOI radius
        aoi.update_entity(2, [200.0, 0.0, 0.0]);
        let d3 = aoi.compute_visibility_delta(1, 50.0);
        assert!(d3.entered.is_empty());
        assert_eq!(d3.left, vec![2]); // Entity 2 left visibility!

        // 4. Entity 2 walks back inside AOI radius to (10m, 0, 0)
        let d4_2 = aoi.update_entity_with_delta(2, [10.0, 0.0, 0.0], 50.0);
        assert_eq!(d4_2.entered, vec![1]); // Entity 2 sees Entity 1
        let d4 = aoi.compute_visibility_delta(1, 50.0);
        assert_eq!(d4.entered, vec![2]); // Entity 1 sees Entity 2 re-entered!
        assert!(d4.left.is_empty());

        // 5. Entity 2 disconnects / despawns
        let remove_delta = aoi.remove_entity(2);
        assert_eq!(remove_delta.left, vec![1]); // Entity 2 no longer sees Entity 1
        let d5 = aoi.compute_visibility_delta(1, 50.0);
        assert_eq!(d5.left, vec![2]); // Entity 1 saw Entity 2 leave
    }

    /// The delta path short-circuits when nothing changed; a later exit must
    /// still be reported, so the retained set cannot go stale over idle ticks.
    #[test]
    fn test_aoi_repeated_idle_ticks_then_exit() {
        let mut aoi = AoiManager3D::new();
        aoi.update_entity(1, [0.0, 0.0, 0.0]);
        aoi.update_entity(2, [10.0, 0.0, 0.0]);

        assert_eq!(aoi.compute_visibility_delta(1, 50.0), VisibilityDelta { entered: vec![2], left: vec![] });
        for _ in 0..8 {
            aoi.update_entity(1, [0.5, 0.0, 0.0]);
            assert_eq!(aoi.compute_visibility_delta(1, 50.0), VisibilityDelta::default());
        }

        aoi.update_entity(2, [400.0, 0.0, 0.0]);
        assert_eq!(aoi.compute_visibility_delta(1, 50.0), VisibilityDelta { entered: vec![], left: vec![2] });
    }

    #[test]
    fn test_aoi_visibility_delta_2d() {
        let mut aoi = AoiManager2D::new();
        aoi.update_entity(10, [0.0, 0.0]);
        aoi.update_entity(20, [30.0, 0.0]);

        // Entity 10 checks 40m radius
        let d1 = aoi.compute_visibility_delta(10, 40.0);
        assert_eq!(d1.entered, vec![20]);

        // Move entity 20 out
        aoi.update_entity(20, [100.0, 100.0]);
        let d2 = aoi.compute_visibility_delta(10, 40.0);
        assert_eq!(d2.left, vec![20]);
    }
}
