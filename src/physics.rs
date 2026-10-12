//! # OpenOxide High-Performance 3D Collision & Kinematics Engine
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
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const UP: Self = Self { x: 0.0, y: 1.0, z: 0.0 };

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
        Self { x: self.x + rhs.x, y: self.y + rhs.y, z: self.z + rhs.z }
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self { x: self.x - rhs.x, y: self.y - rhs.y, z: self.z - rhs.z }
    }
}

impl std::ops::Mul<f64> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self { x: self.x * rhs, y: self.y * rhs, z: self.z * rhs }
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
        pt.x >= self.min.x && pt.x <= self.max.x &&
        pt.y >= self.min.y && pt.y <= self.max.y &&
        pt.z >= self.min.z && pt.z <= self.max.z
    }

    #[inline]
    pub fn closest_point(&self, pt: Vec3) -> Vec3 {
        Vec3::new(
            pt.x.max(self.min.x).min(self.max.y),
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

/// Authoritative Historical Ring-Buffer for Server-Side Lag Compensation (reference Fusion Killer)
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
        if self.history.is_empty() {
            return None;
        }

        // 1. Direct tick match
        if let Some(snap) = self.history.iter().find(|s| s.tick == shoot_tick) {
            return Some(snap);
        }

        // 2. Nearest tick within bounded historical window
        let oldest = self.history.front().unwrap().tick;
        let _newest = self.history.back().unwrap().tick;
        if shoot_tick < oldest.saturating_sub(60) {
            // Far too old (reject ancient packets / anti-cheat)
            return None;
        }

        self.history.iter().min_by_key(|s| {
            (s.tick as i64 - shoot_tick as i64).abs()
        })
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
    pub lag_comp: std::sync::RwLock<LagCompensator>,
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
            lag_comp: std::sync::RwLock::new(LagCompensator::default()),
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
        self.obstacles.push(ObstacleCollider {
            id,
            name: name.to_string(),
            bounds,
            height: bounds.max.y,
            is_walkable_roof: walkable_roof,
        });
    }

    /// Returns the ground or rooftop surface height directly underneath (x, z)
    pub fn get_surface_height(&self, px: f64, py: f64, pz: f64, radius: f64) -> f64 {
        let mut surface_y = 0.0;
        let r = radius * 0.85;

        for obs in &self.obstacles {
            if obs.is_walkable_roof {
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
            }
        }

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

        // 1. Resolve horizontal wall collisions against obstacles
        for obs in &self.obstacles {
            // Only collide horizontally with walls if player is below the roof level
            if cur_y < obs.height - 1.0 {
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

        for obs in &self.obstacles {
            if let Some(mut hit) = obs.bounds.ray_intersect(origin, norm_dir, nearest_dist) {
                if hit.distance < nearest_dist {
                    nearest_dist = hit.distance;
                    hit.collider_id = obs.id;
                    closest_hit = Some(hit);
                }
            }
        }

        closest_hit
    }

    /// Record an active dynamic entity hitbox into the authoritative lag compensation history
    pub fn record_entity_hitbox(&self, tick: u64, timestamp_ms: u64, entity_id: u32, kind: u32, bounds: Aabb) {
        if let Ok(mut lc) = self.lag_comp.write() {
            lc.record_entity(
                tick,
                timestamp_ms,
                HistoricalEntity {
                    entity_id,
                    kind,
                    bounds,
                },
            );
        }
    }

    /// Fast authoritative lag-compensated hitscan raycast (reference Fusion Server Rewind)
    ///
    /// 1. Rewinds time to `shoot_tick`
    /// 2. Tests ray intersection against historical entity hitboxes
    /// 3. Tests ray intersection against static map obstacles (anti-wallhack verification)
    /// 4. If an obstacle is closer than the entity, shot was blocked by wall (hit_type = 2)
    /// 5. If entity is hit with clear line of sight, hit is confirmed (hit_type = 1)
    pub fn lag_compensated_raycast(
        &self,
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
        let lc = self.lag_comp.read().ok()?;
        let snapshot = lc.find_snapshot(shoot_tick)?;

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

    #[test]
    fn test_raycast_hit() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);
        world.add_box_obstacle(1, "bunker", Vec3::new(0.0, 10.0, 50.0), Vec3::new(20.0, 20.0, 20.0), true);

        let hit = world.raycast(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 100.0);
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert_eq!(h.collider_id, 1);
        assert!((h.distance - 40.0).abs() < 1e-4);
    }

    #[test]
    fn test_move_and_slide() {
        let mut world = PhysicsWorld3D::new(400.0, 400.0);
        world.add_box_obstacle(1, "bunker", Vec3::new(0.0, 10.0, 0.0), Vec3::new(20.0, 20.0, 20.0), true);

        // Try walking into the box
        let res = world.move_and_slide(0.0, 0.0, -15.0, 0.0, 0.0, 10.0, 3.6, 1.0);
        assert!(res.is_walled);
        assert!(res.new_z <= -10.0 - 3.6 + 1e-4);
    }

    #[test]
    fn test_lag_compensation_rewind_success() {
        let world = PhysicsWorld3D::new(400.0, 400.0);

        // Target player (ID 42) is at (0, 5, 40) at Tick 100
        let box_tick_100 = Aabb::from_center_size(Vec3::new(0.0, 5.0, 40.0), Vec3::new(4.0, 4.0, 4.0));
        world.record_entity_hitbox(100, 1000, 42, 1, box_tick_100);

        // Target player moves to (0, 5, 80) at Tick 105
        let box_tick_105 = Aabb::from_center_size(Vec3::new(0.0, 5.0, 80.0), Vec3::new(4.0, 4.0, 4.0));
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
        world.add_box_obstacle(99, "concrete_wall", Vec3::new(0.0, 5.0, 20.0), Vec3::new(20.0, 10.0, 2.0), false);

        // Target player is behind the wall at z = 40
        let target_box = Aabb::from_center_size(Vec3::new(0.0, 5.0, 40.0), Vec3::new(4.0, 4.0, 4.0));
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
