use rerecast::{Config, ConfigBuilder};

#[derive(Debug, Clone)]
pub struct NavMesh3D {
    pub config: Config,
    pub baked_data: Vec<u8>,
}

impl NavMesh3D {
    pub fn new() -> Self {
        Self {
            config: ConfigBuilder::default().build(),
            baked_data: Vec::new(),
        }
    }

    pub fn from_baked(data: Vec<u8>) -> Self {
        Self {
            config: ConfigBuilder::default().build(),
            baked_data: data,
        }
    }

    pub fn is_in_bounds(&self, point: [f32; 3]) -> bool {
        let aabb = &self.config.aabb;
        point[0] >= aabb.min.x
            && point[0] <= aabb.max.x
            && point[1] >= aabb.min.y
            && point[1] <= aabb.max.y
            && point[2] >= aabb.min.z
            && point[2] <= aabb.max.z
    }
}

impl Default for NavMesh3D {
    fn default() -> Self {
        Self::new()
    }
}
