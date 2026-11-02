use serde::{Deserialize, Serialize};

use super::Aabb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObstacleCollider {
    pub id: u32,
    pub name: String,
    pub bounds: Aabb,
    pub height: f64,
    pub is_walkable_roof: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlideResult {
    pub new_x: f64,
    pub new_y: f64,
    pub new_z: f64,
    pub is_grounded: bool,
    pub is_walled: bool,
}

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
