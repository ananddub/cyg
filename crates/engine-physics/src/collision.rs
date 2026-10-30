//! # CYG High-Performance 3D Collision & Kinematics Engine
//!
//! Provides deterministic 3D AABB obstacle collisions, Kinematic Character
//! Controllers (move_and_slide), and Continuous Collision Detection (CCD)
//! raycasts for bullets and hitscans with zero heap allocation in tick loops.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UP: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    #[inline]
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[inline]
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 1e-6 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            Self::ZERO
        }
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Mul<f64> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl std::ops::AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

/// 3D Axis-Aligned Bounding Box
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    #[inline]
    pub fn from_center_size(center: Vec3, size: Vec3) -> Self {
        let half = size * 0.5;
        Self {
            min: Vec3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: Vec3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    #[inline]
    pub fn contains_point(&self, pt: Vec3) -> bool {
        pt.x >= self.min.x
            && pt.x <= self.max.x
            && pt.y >= self.min.y
            && pt.y <= self.max.y
            && pt.z >= self.min.z
            && pt.z <= self.max.z
    }

    #[inline]
    pub fn closest_point(&self, pt: Vec3) -> Vec3 {
        Vec3::new(
            pt.x.max(self.min.x).min(self.max.x),
            pt.y.max(self.min.y).min(self.max.y),
            pt.z.max(self.min.z).min(self.max.z),
        )
    }

    /// Fast slab-method Ray vs AABB intersection
    pub fn ray_intersect(&self, origin: Vec3, dir: Vec3, max_dist: f64) -> Option<RayHit> {
        let mut tmin = 0.0;
        let mut tmax = max_dist;
        let mut normal = Vec3::ZERO;

        // X axis
        if dir.x.abs() > 1e-8 {
            let inv_d = 1.0 / dir.x;
            let mut t1 = (self.min.x - origin.x) * inv_d;
            let mut t2 = (self.max.x - origin.x) * inv_d;
            let mut n = Vec3::new(-1.0, 0.0, 0.0);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = Vec3::new(1.0, 0.0, 0.0);
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        } else if origin.x < self.min.x || origin.x > self.max.x {
            return None;
        }

        // Y axis
        if dir.y.abs() > 1e-8 {
            let inv_d = 1.0 / dir.y;
            let mut t1 = (self.min.y - origin.y) * inv_d;
            let mut t2 = (self.max.y - origin.y) * inv_d;
            let mut n = Vec3::new(0.0, -1.0, 0.0);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = Vec3::new(0.0, 1.0, 0.0);
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        } else if origin.y < self.min.y || origin.y > self.max.y {
            return None;
        }

        // Z axis
        if dir.z.abs() > 1e-8 {
            let inv_d = 1.0 / dir.z;
            let mut t1 = (self.min.z - origin.z) * inv_d;
            let mut t2 = (self.max.z - origin.z) * inv_d;
            let mut n = Vec3::new(0.0, 0.0, -1.0);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = Vec3::new(0.0, 0.0, 1.0);
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        } else if origin.z < self.min.z || origin.z > self.max.z {
            return None;
        }

        if tmin <= max_dist && tmin >= 0.0 {
            let point = origin + dir * tmin;
            Some(RayHit {
                hit: true,
                point,
                normal,
                distance: tmin,
                collider_id: 0,
            })
        } else {
            None
        }
    }
}

/// Raycast Hit Information
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RayHit {
    pub hit: bool,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f64,
    pub collider_id: u32,
}

/// Static Obstacle in the 3D World
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObstacleCollider {
    pub id: u32,
    pub name: String,
    pub bounds: Aabb,
    pub height: f64,
    pub is_walkable_roof: bool,
}

/// Kinematic Character Movement Result
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlideResult {
    pub new_x: f64,
    pub new_y: f64,
    pub new_z: f64,
    pub is_grounded: bool,
    pub is_walled: bool,
}

/// Map Configuration File Format (JSON compatible with Unity / Godot / Blender exports)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    pub arena_width: f64,
    pub arena_depth: f64,
    pub obstacles: Vec<MapObstacleDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapObstacleDef {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    #[serde(default = "default_walkable")]
    pub walkable_roof: bool,
}

fn default_walkable() -> bool {
    true
}

/// Obstacles examined by one broadphase query before it is cheaper to just
/// scan the whole obstacle list.
const MAX_GRID_CANDIDATES: usize = 128;

/// Stack-allocated result of one broadphase query: unique obstacle indices in
/// ascending order, so any iteration order matches the old full scan.
struct Candidates {
    ids: [u32; MAX_GRID_CANDIDATES],
    len: usize,
    /// The query rect overlapped more cells than the window holds; the caller
    /// must fall back to scanning every obstacle.
    overflow: bool,
}

impl Candidates {
    #[inline]
    fn new() -> Self {
        Self {
            ids: [0; MAX_GRID_CANDIDATES],
            len: 0,
            overflow: false,
        }
    }

    /// Insert keeping the window sorted, dropping duplicates.
    #[inline]
    fn add(&mut self, id: u32) {
        if self.overflow {
            return;
        }
        let len = self.len;
        if len == self.ids.len() {
            self.overflow = true;
            return;
        }
        let pos = self.ids[..len].partition_point(|&x| x < id);
        if pos < len && self.ids[pos] == id {
            return;
        }
        self.ids.copy_within(pos..len, pos + 1);
        self.ids[pos] = id;
        self.len = len + 1;
    }

    #[inline]
    fn as_slice(&self) -> &[u32] {
        &self.ids[..self.len]
    }
}

/// Uniform XZ grid over the arena's static obstacles.
///
/// Obstacle geometry is loaded once and never changes, but every tick asked
/// `move_and_slide`, `get_surface_height` and `raycast` to test *all* obstacles
/// — O(players x obstacles) per tick. Each obstacle is registered in the cells
/// its footprint touches, so a query only sees the few obstacles around it.
/// Cell lists hold obstacle indices in ascending order and [`Candidates`]
/// deduplicates into ascending order, so the visited sequence is a subsequence
/// of the old full scan and resolution results (and therefore state checksums)
/// are unchanged.
#[derive(Debug, Clone)]
struct StaticObstacleGrid {
    cell_size: f64,
    half_x: f64,
    half_z: f64,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

impl StaticObstacleGrid {
    /// No cell finer than 16 m, and at most 256 cells per axis so an absurdly
    /// large arena cannot blow up the dense table.
    fn new(half_x: f64, half_z: f64) -> Self {
        let extent = (half_x.max(half_z) * 2.0).max(1.0);
        let cell_size = 16.0f64.max(extent / 256.0);
        let cols = ((half_x * 2.0) / cell_size).ceil() as usize + 1;
        let rows = ((half_z * 2.0) / cell_size).ceil() as usize + 1;
        Self {
            cell_size,
            half_x,
            half_z,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
        }
    }

    /// Out-of-arena coordinates clamp to the border cells. Inserts and queries
    /// clamp the same way, so a query can only ever over-approximate.
    #[inline]
    fn col_of(&self, x: f64) -> usize {
        (((x + self.half_x) / self.cell_size).max(0.0) as usize).min(self.cols - 1)
    }

    #[inline]
    fn row_of(&self, z: f64) -> usize {
        (((z + self.half_z) / self.cell_size).max(0.0) as usize).min(self.rows - 1)
    }

    fn insert(&mut self, obs_index: usize, bounds: &Aabb) {
        let id = obs_index as u32;
        let (c0, c1) = (self.col_of(bounds.min.x), self.col_of(bounds.max.x));
        let (r0, r1) = (self.row_of(bounds.min.z), self.row_of(bounds.max.z));
        for row in r0..=r1 {
            let start = row * self.cols;
            for col in c0..=c1 {
                self.cells[start + col].push(id);
            }
        }
    }

    /// The obstacles whose footprint overlaps `[min_x..max_x] x [min_z..max_z]`.
    fn query(&self, min_x: f64, max_x: f64, min_z: f64, max_z: f64) -> Candidates {
        let mut out = Candidates::new();
        let (lo_x, hi_x) = (min_x.min(max_x), min_x.max(max_x));
        let (lo_z, hi_z) = (min_z.min(max_z), min_z.max(max_z));
        let (c0, c1) = (self.col_of(lo_x), self.col_of(hi_x));
        let (r0, r1) = (self.row_of(lo_z), self.row_of(hi_z));
        for row in r0..=r1 {
            let start = row * self.cols;
            for col in c0..=c1 {
                for &id in &self.cells[start + col] {
                    out.add(id);
                }
            }
        }
        out
    }
}

/// Wall resolutions allowed before `move_and_slide` gives up re-querying the
/// grid around the moving probe.
const MAX_SLIDE_PASSES: usize = 8;

/// Historical Entity Hitbox for Lag Compensation / Server Rewind
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoricalEntity {
    pub entity_id: u32,
    pub kind: u32, // 1 = Body, 2 = Head/Critical, 3 = Shield/Vehicle
    pub bounds: Aabb,
}

/// Snapshot of all active dynamic entity hitboxes at a specific simulation tick
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickSnapshot {
    pub tick: u64,
    pub timestamp_ms: u64,
    pub entities: Vec<HistoricalEntity>,
}

/// Authoritative Historical Ring-Buffer for Server-Side Lag Compensation
#[derive(Debug, Clone)]
pub struct LagCompensator {
    pub history: std::collections::VecDeque<TickSnapshot>,
    pub max_ticks: usize,
}

impl Default for LagCompensator {
    fn default() -> Self {
        Self::new(128) // 128 ticks = ~2.13 seconds of history at 60Hz
    }
}

impl LagCompensator {
    pub fn new(max_ticks: usize) -> Self {
        Self {
            history: std::collections::VecDeque::with_capacity(max_ticks),
            max_ticks: max_ticks.max(16),
        }
    }

    /// Record an active entity hitbox for the specified server tick
    pub fn record_entity(&mut self, tick: u64, timestamp_ms: u64, entity: HistoricalEntity) {
        if let Some(last) = self.history.back_mut() {
            if last.tick == tick {
                last.entities.push(entity);
                return;
            }
        }

        // New tick started: ensure bounded memory (ring buffer behavior)
        if self.history.len() >= self.max_ticks {
            self.history.pop_front();
        }
        self.history.push_back(TickSnapshot {
            tick,
            timestamp_ms,
            entities: vec![entity],
        });
    }

    /// Retrieve the historical snapshot corresponding to client shoot tick (with bounds clamping)
    pub fn find_snapshot(&self, shoot_tick: u64) -> Option<&TickSnapshot> {
        // Snapshots are appended as ticks advance, so `history` is tick-sorted
        // and the lookup is a binary search rather than a scan of ~128 entries.
        let count = self.history.len();
        if count == 0 {
            return None;
        }

        // First snapshot whose tick is >= `shoot_tick`.
        let (mut lo, mut hi) = (0usize, count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.history[mid].tick < shoot_tick {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        // 1. Direct tick match. This is the first entry with that tick, exactly
        // what the old linear `find` returned.
        if lo < count && self.history[lo].tick == shoot_tick {
            return Some(&self.history[lo]);
        }

        // 2. Nearest tick within bounded historical window
        let oldest = self.history.front().unwrap().tick;
        if shoot_tick < oldest.saturating_sub(60) {
            // Far too old (reject ancient packets / anti-cheat)
            return None;
        }

        let (prev, next) = if lo == 0 {
            (None, Some(&self.history[0]))
        } else if lo == count {
            (Some(&self.history[count - 1]), None)
        } else {
            (Some(&self.history[lo - 1]), Some(&self.history[lo]))
        };
        match (prev, next) {
            // A tie goes to the older tick, like `min_by_key`'s first winner.
            (Some(p), Some(n)) => {
                let dp = (p.tick as i64 - shoot_tick as i64).abs();
                let dn = (n.tick as i64 - shoot_tick as i64).abs();
                Some(if dp <= dn { p } else { n })
            }
            (Some(p), None) => Some(p),
            (None, Some(n)) => Some(n),
            (None, None) => None,
        }
    }
}

/// Result of a lag-compensated rewind hitscan query
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LagCompensatedHit {
    pub point: Vec3,
    pub distance: f64,
    pub entity_id: u32,
    pub kind: u32,
    pub collider_id: u32,
    pub hit_type: u32, // 1 = dynamic entity hit, 2 = static obstacle hit, 0 = miss
}

/// The 3D Physics & Kinematics World
#[derive(Debug)]
pub struct PhysicsWorld3D {
    pub arena_half_width: f64,
    pub arena_half_depth: f64,
    pub obstacles: Vec<ObstacleCollider>,
    pub lag_comp: LagCompensator,
    /// Broadphase over `obstacles`, kept in sync by `add_box_obstacle`.
    grid: StaticObstacleGrid,
}

impl Clone for PhysicsWorld3D {
    /// Copies map geometry but starts a fresh lag-compensation history —
    /// each room owns its own rewind timeline.
    fn clone(&self) -> Self {
        Self {
            arena_half_width: self.arena_half_width,
            arena_half_depth: self.arena_half_depth,
            obstacles: self.obstacles.clone(),
            grid: self.grid.clone(),
            lag_comp: LagCompensator::default(),
        }
    }
}

impl Default for PhysicsWorld3D {
    fn default() -> Self {
        Self::new(440.0, 440.0)
    }
}

impl PhysicsWorld3D {
    pub fn new(half_width: f64, half_depth: f64) -> Self {
        Self {
            arena_half_width: half_width,
            arena_half_depth: half_depth,
            obstacles: Vec::new(),
            grid: StaticObstacleGrid::new(half_width, half_depth),
            lag_comp: LagCompensator::default(),
        }
    }

    /// Load map geometry from JSON string
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        let cfg: MapConfig = serde_json::from_str(json_str)?;
        let mut world = Self::new(cfg.arena_width * 0.5, cfg.arena_depth * 0.5);
        for obs in cfg.obstacles {
            world.add_box_obstacle(
                obs.id,
                &obs.name,
                Vec3::new(obs.x, obs.y, obs.z),
                Vec3::new(obs.w, obs.h, obs.d),
                obs.walkable_roof,
            );
        }
        Ok(world)
    }

    /// Add a box obstacle
    pub fn add_box_obstacle(
        &mut self,
        id: u32,
        name: &str,
        center: Vec3,
        size: Vec3,
        walkable_roof: bool,
    ) {
        let bounds = Aabb::from_center_size(center, size);
        let index = self.obstacles.len();
        self.obstacles.push(ObstacleCollider {
            id,
            name: name.to_string(),
            bounds,
            height: bounds.max.y,
            is_walkable_roof: walkable_roof,
        });
        self.grid.insert(index, &bounds);
    }

    /// Visit `f` for every obstacle overlapping the XZ rect, in obstacle order.
    /// Falls back to a full scan when a query window overflows.
    #[inline]
    fn for_each_in_rect(
        &self,
        min_x: f64,
        max_x: f64,
        min_z: f64,
        max_z: f64,
        mut f: impl FnMut(&ObstacleCollider),
    ) {
        let found = self.grid.query(min_x, max_x, min_z, max_z);
        if found.overflow {
            for obs in &self.obstacles {
                f(obs);
            }
            return;
        }
        for &i in found.as_slice() {
            f(&self.obstacles[i as usize]);
        }
    }

    /// Returns the ground or rooftop surface height directly underneath (x, z)
    pub fn get_surface_height(&self, px: f64, py: f64, pz: f64, radius: f64) -> f64 {
        let mut surface_y = 0.0;
        let r = radius * 0.85;

        // A roof counts iff its footprint touches [px-r..px+r] x [pz-r..pz+r],
        // which is exactly the rect the grid is queried with.
        self.for_each_in_rect(px - r, px + r, pz - r, pz + r, |obs| {
            if !obs.is_walkable_roof {
                return;
            }
            let min_x = obs.bounds.min.x - r;
            let max_x = obs.bounds.max.x + r;
            let min_z = obs.bounds.min.z - r;
            let max_z = obs.bounds.max.z + r;

            if px >= min_x && px <= max_x && pz >= min_z && pz <= max_z {
                // Check if player is approaching or standing at rooftop level
                if py >= obs.height - 2.5 && obs.height > surface_y {
                    surface_y = obs.height;
                }
            }
        });

        surface_y
    }

    /// Kinematic Character Movement: wall sliding along contact normals
    pub fn move_and_slide(
        &self,
        px: f64,
        py: f64,
        pz: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        radius: f64,
        dt: f64,
    ) -> SlideResult {
        let mut cur_x = px + vx * dt;
        let mut cur_y = py;
        let mut cur_z = pz + vz * dt;
        let mut is_walled = false;

        // 1. Resolve horizontal wall collisions against nearby obstacles.
        // A push-out relocates the probe, so keep re-querying around the new
        // position until it stops moving.
        let probe = radius * 2.0;
        for _ in 0..MAX_SLIDE_PASSES {
            let before = (cur_x, cur_z);
            self.for_each_in_rect(
                cur_x - probe,
                cur_x + probe,
                cur_z - probe,
                cur_z + probe,
                |obs| {
                    // Only collide horizontally with walls if player is below the roof level
                    if cur_y >= obs.height - 1.0 {
                        return;
                    }
                    let cx = cur_x.max(obs.bounds.min.x).min(obs.bounds.max.x);
                    let cz = cur_z.max(obs.bounds.min.z).min(obs.bounds.max.z);
                    let dx = cur_x - cx;
                    let dz = cur_z - cz;
                    let dist_sq = dx * dx + dz * dz;

                    if dist_sq < radius * radius {
                        is_walled = true;
                        let dist = dist_sq.sqrt();
                        if dist > 1e-4 {
                            let overlap = radius - dist;
                            cur_x += (dx / dist) * overlap;
                            cur_z += (dz / dist) * overlap;
                        } else {
                            // Center is inside box: push out to closest edge
                            let dl = cur_x - obs.bounds.min.x;
                            let dr = obs.bounds.max.x - cur_x;
                            let dt_z = cur_z - obs.bounds.min.z;
                            let db_z = obs.bounds.max.z - cur_z;
                            let min_d = dl.min(dr).min(dt_z).min(db_z);

                            if (min_d - dl).abs() < 1e-6 {
                                cur_x = obs.bounds.min.x - radius;
                            } else if (min_d - dr).abs() < 1e-6 {
                                cur_x = obs.bounds.max.x + radius;
                            } else if (min_d - dt_z).abs() < 1e-6 {
                                cur_z = obs.bounds.min.z - radius;
                            } else {
                                cur_z = obs.bounds.max.z + radius;
                            }
                        }
                    }
                },
            );
            if (cur_x, cur_z) == before {
                break;
            }
        }

        // 2. Arena boundary clamping
        let bound_x = self.arena_half_width - radius;
        let bound_z = self.arena_half_depth - radius;
        cur_x = cur_x.max(-bound_x).min(bound_x);
        cur_z = cur_z.max(-bound_z).min(bound_z);

        // 3. Vertical surface / ground integration
        let surface_y = self.get_surface_height(cur_x, cur_y, cur_z, radius);
        let mut is_grounded = false;

        if vy <= 0.0 && cur_y <= surface_y + 0.1 {
            cur_y = surface_y;
            is_grounded = true;
        }

        SlideResult {
            new_x: cur_x,
            new_y: cur_y,
            new_z: cur_z,
            is_grounded,
            is_walled,
        }
    }

    /// Fast continuous raycast against all static obstacles
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f64) -> Option<RayHit> {
        let norm_dir = dir.normalize();
        let mut closest_hit: Option<RayHit> = None;
        let mut nearest_dist = max_dist;

        // Anything the segment can touch has a footprint overlapping the
        // segment's XZ bounding box, so that box is the broadphase query rect.
        let far = origin + norm_dir * max_dist;
        self.for_each_in_rect(origin.x, far.x, origin.z, far.z, |obs| {
            if let Some(mut hit) = obs.bounds.ray_intersect(origin, norm_dir, nearest_dist) {
                if hit.distance < nearest_dist {
                    nearest_dist = hit.distance;
                    hit.collider_id = obs.id;
                    closest_hit = Some(hit);
                }
            }
        });

        closest_hit
    }

    /// Record an active dynamic entity hitbox into the authoritative lag compensation history
    pub fn record_entity_hitbox(
        &mut self,
        tick: u64,
        timestamp_ms: u64,
        entity_id: u32,
        kind: u32,
        bounds: Aabb,
    ) {
        self.lag_comp.record_entity(
            tick,
            timestamp_ms,
            HistoricalEntity {
                entity_id,
                kind,
                bounds,
            },
        );
    }

    /// Fast authoritative lag-compensated hitscan raycast with server rewind
    ///
    /// 1. Rewinds time to `shoot_tick`
    /// 2. Tests ray intersection against historical entity hitboxes
    /// 3. Tests ray intersection against static map obstacles (anti-wallhack verification)
    /// 4. If an obstacle is closer than the entity, shot was blocked by wall (hit_type = 2)
    /// 5. If entity is hit with clear line of sight, hit is confirmed (hit_type = 1)
    pub fn lag_compensated_raycast(
        &mut self,
        shoot_tick: u64,
        origin: Vec3,
        dir: Vec3,
        max_dist: f64,
    ) -> Option<LagCompensatedHit> {
        let norm_dir = dir.normalize();

        // 1. Raycast static world obstacles first (line-of-sight & wall occluder)
        let static_hit = self.raycast(origin, norm_dir, max_dist);
        let max_range = match &static_hit {
            Some(h) => h.distance,
            None => max_dist,
        };

        // 2. Query historical snapshot at `shoot_tick`
        let snapshot = self.lag_comp.find_snapshot(shoot_tick)?;

        let mut closest_entity: Option<LagCompensatedHit> = None;
        let mut nearest_dist = max_range;

        for ent in &snapshot.entities {
            if let Some(hit) = ent.bounds.ray_intersect(origin, norm_dir, nearest_dist) {
                if hit.distance < nearest_dist {
                    nearest_dist = hit.distance;
                    closest_entity = Some(LagCompensatedHit {
                        point: hit.point,
                        distance: hit.distance,
                        entity_id: ent.entity_id,
                        kind: ent.kind,
                        collider_id: 0,
                        hit_type: 1, // Entity hit
                    });
                }
            }
        }

        // If entity was hit in front of any walls: return entity hit!
        if let Some(ent_hit) = closest_entity {
            return Some(ent_hit);
        }

        // Otherwise, if a static wall was hit within range: return wall hit
        if let Some(wall) = static_hit {
            return Some(LagCompensatedHit {
                point: wall.point,
                distance: wall.distance,
                entity_id: 0,
                kind: 0,
                collider_id: wall.collider_id,
                hit_type: 2, // Static wall hit
            });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rewind lookup is a binary search over a tick-sorted ring buffer;
    /// every tick in and around the window must resolve to the same snapshot the
    /// original linear scan picked.
    #[test]
    fn test_find_snapshot_binary_search_matches_linear_scan() {
        let mut comp = LagCompensator::new(64);
        let mut seed = 0x9E3779B97F4A7C15u64;
        let mut tick = 0u64;
        for i in 0..200u64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            tick += 1 + seed % 3;
            comp.record_entity(
                tick,
                i,
                HistoricalEntity {
                    entity_id: i as u32,
                    kind: 1,
                    bounds: Aabb {
                        min: Vec3::ZERO,
                        max: Vec3::ZERO,
                    },
                },
            );
        }

        let linear = |shoot_tick: u64| -> Option<u64> {
            if let Some(s) = comp.history.iter().find(|s| s.tick == shoot_tick) {
                return Some(s.tick);
            }
            let oldest = comp.history.front().unwrap().tick;
            if shoot_tick < oldest.saturating_sub(60) {
                return None;
            }
            comp.history
                .iter()
                .min_by_key(|s| (s.tick as i64 - shoot_tick as i64).abs())
                .map(|s| s.tick)
        };

        for t in tick.saturating_sub(140)..=tick + 40 {
            assert_eq!(
                comp.find_snapshot(t).map(|s| s.tick),
                linear(t),
                "shoot_tick={t}"
            );
        }
    }

    #[test]
    fn test_raycast_hit() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);
        world.add_box_obstacle(
            1,
            "bunker",
            Vec3::new(0.0, 10.0, 50.0),
            Vec3::new(20.0, 20.0, 20.0),
            true,
        );

        let hit = world.raycast(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 100.0);
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert_eq!(h.collider_id, 1);
        assert!((h.distance - 40.0).abs() < 1e-4);
    }

    /// The broadphase may only *filter* obstacles out of a query, never reorder
    /// or drop one that a full scan would have used: grid results must equal the
    /// pre-broadphase linear scans on every random map/query.
    #[test]
    fn test_broadphase_grid_matches_full_scan() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);
        let mut seed = 0x243F6A8885A308D3u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 800) as f64 - 400.0
        };
        for i in 0..60u32 {
            let size = Vec3::new(4.0 + (i % 7) as f64 * 8.0, 10.0, 4.0 + (i % 5) as f64 * 8.0);
            world.add_box_obstacle(i, "box", Vec3::new(rnd(), 5.0, rnd()), size, i % 3 == 0);
        }

        for _ in 0..400 {
            let (px, pz) = (rnd(), rnd());
            let radius = 2.0 + ((seed >> 13) % 40) as f64;
            let r = radius * 0.85;
            let py = 5.0;

            let mut expected_surface = 0.0;
            for obs in &world.obstacles {
                if obs.is_walkable_roof
                    && px >= obs.bounds.min.x - r
                    && px <= obs.bounds.max.x + r
                    && pz >= obs.bounds.min.z - r
                    && pz <= obs.bounds.max.z + r
                    && py >= obs.height - 2.5
                    && obs.height > expected_surface
                {
                    expected_surface = obs.height;
                }
            }
            assert_eq!(
                world.get_surface_height(px, py, pz, radius),
                expected_surface
            );

            let dir = Vec3::new(rnd(), 0.0, rnd());
            let norm = dir.normalize();
            let origin = Vec3::new(px, py, pz);
            let max_dist = 120.0;
            let mut expected: Option<RayHit> = None;
            let mut nearest = max_dist;
            for obs in &world.obstacles {
                if let Some(mut hit) = obs.bounds.ray_intersect(origin, norm, nearest) {
                    if hit.distance < nearest {
                        nearest = hit.distance;
                        hit.collider_id = obs.id;
                        expected = Some(hit);
                    }
                }
            }
            assert_eq!(
                world.raycast(origin, dir, max_dist),
                expected,
                "raycast at {px},{pz}"
            );
        }
    }

    #[test]
    fn test_move_and_slide() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);
        world.add_box_obstacle(
            1,
            "bunker",
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::new(20.0, 20.0, 20.0),
            true,
        );

        // Try walking into the box
        let res = world.move_and_slide(0.0, 0.0, -15.0, 0.0, 0.0, 10.0, 3.6, 1.0);
        assert!(res.is_walled);
        assert!(res.new_z <= -10.0 - 3.6 + 1e-4);
    }

    #[test]
    fn test_lag_compensation_rewind_success() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);

        // Target player (ID 42) is at (0, 5, 40) at Tick 100
        let box_tick_100 =
            Aabb::from_center_size(Vec3::new(0.0, 5.0, 40.0), Vec3::new(4.0, 4.0, 4.0));
        world.record_entity_hitbox(100, 1000, 42, 1, box_tick_100);

        // Target player moves to (0, 5, 80) at Tick 105
        let box_tick_105 =
            Aabb::from_center_size(Vec3::new(0.0, 5.0, 80.0), Vec3::new(4.0, 4.0, 4.0));
        world.record_entity_hitbox(105, 1083, 42, 1, box_tick_105);

        // Shooter fires at Tick 100 aiming towards (0, 5, 40)
        let ray_origin = Vec3::new(0.0, 5.0, 0.0);
        let ray_dir = Vec3::new(0.0, 0.0, 1.0);

        // 1. Rewind to Tick 100: Hits target at z ~ 38!
        let hit_100 = world.lag_compensated_raycast(100, ray_origin, ray_dir, 100.0);
        assert!(hit_100.is_some(), "Rewind to tick 100 must hit target");
        let h100 = hit_100.unwrap();
        assert_eq!(h100.entity_id, 42);
        assert_eq!(h100.hit_type, 1);
        assert!((h100.distance - 38.0).abs() < 1e-4);

        // 2. Query at Tick 105 aiming with max_dist = 50: Target moved to 80, so should miss!
        let hit_105 = world.lag_compensated_raycast(105, ray_origin, ray_dir, 50.0);
        assert!(hit_105.is_none(), "Target moved out of range at tick 105");
    }

    #[test]
    fn test_lag_compensation_wall_occlusion() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);
        // Concrete wall at z = 20
        world.add_box_obstacle(
            99,
            "concrete_wall",
            Vec3::new(0.0, 5.0, 20.0),
            Vec3::new(20.0, 10.0, 2.0),
            false,
        );

        // Target player is behind the wall at z = 40
        let target_box =
            Aabb::from_center_size(Vec3::new(0.0, 5.0, 40.0), Vec3::new(4.0, 4.0, 4.0));
        world.record_entity_hitbox(50, 500, 77, 1, target_box);

        // Shooter fires from z = 0 towards target at z = 40
        let ray_origin = Vec3::new(0.0, 5.0, 0.0);
        let ray_dir = Vec3::new(0.0, 0.0, 1.0);

        let hit = world.lag_compensated_raycast(50, ray_origin, ray_dir, 100.0);
        assert!(hit.is_some());
        let h = hit.unwrap();
        // Must hit the wall (hit_type 2, collider 99), NOT the player behind it!
        assert_eq!(h.hit_type, 2, "Wall must occlude line of sight");
        assert_eq!(h.collider_id, 99);
        assert_eq!(h.entity_id, 0);
    }
}
