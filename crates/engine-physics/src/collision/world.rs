use super::grid::StaticObstacleGrid;
use super::lag::LagCompensator;
use super::map::{MapConfig, ObstacleCollider};
use super::{Aabb, Vec3};

#[derive(Debug)]
pub struct PhysicsWorld3D {
    pub arena_half_width: f64,
    pub arena_half_depth: f64,
    pub obstacles: Vec<ObstacleCollider>,
    pub lag_comp: LagCompensator,

    grid: StaticObstacleGrid,
}

impl Clone for PhysicsWorld3D {
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

    #[inline]
    pub(super) fn for_each_in_rect(
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
}
