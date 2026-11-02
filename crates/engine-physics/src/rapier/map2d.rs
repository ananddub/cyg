use rapier2d::prelude as r2d;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MapConfig2D {
    pub arena_width: f32,
    pub arena_height: f32,

    #[serde(default)]
    pub gravity: Option<[f32; 2]>,
    #[serde(default)]
    pub obstacles: Vec<MapObstacleDef2D>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MapObstacleDef2D {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub x: f32,
    pub y: f32,

    #[serde(default)]
    pub w: f32,

    #[serde(default)]
    pub h: f32,

    #[serde(default)]
    pub r: f32,

    #[serde(default)]
    pub rotation: f32,
}

impl super::RapierWorld2D {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        let cfg: MapConfig2D = serde_json::from_str(json_str)?;
        let [gx, gy] = cfg.gravity.unwrap_or([0.0, -9.81]);
        let mut world = Self::new(gx, gy);
        world.arena_width = cfg.arena_width;
        world.arena_height = cfg.arena_height;
        for obs in cfg.obstacles {
            let handle = world.create_body(1, obs.x, obs.y);
            if obs.r > 0.0 {
                world.add_circle_collider(handle, obs.r, false, 0.0, 0.0);
            } else {
                let col = r2d::ColliderBuilder::cuboid(obs.w * 0.5, obs.h * 0.5)
                    .rotation(obs.rotation)
                    .friction(0.0)
                    .build();
                world
                    .collider_set
                    .insert_with_parent(col, handle, &mut world.rigid_body_set);
            }
        }
        Ok(world)
    }
}
