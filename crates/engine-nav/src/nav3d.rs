use rerecast::{Config, ConfigBuilder};

/// 3D Navigation Mesh generator and query structure backed by Recast (rerecast)
#[derive(Debug, Clone)]
pub struct NavMesh3D {
    pub config: Config,
    pub baked_data: Vec<u8>,
}

impl NavMesh3D {
    /// Creates a 3D NavMesh configuration with human-sized agent defaults.
    pub fn new() -> Self {
        Self {
            config: ConfigBuilder::default().build(),
            baked_data: Vec::new(),
        }
    }

    /// Creates a 3D NavMesh from pre-baked binary bytes.
    pub fn from_baked(data: Vec<u8>) -> Self {
        Self {
            config: ConfigBuilder::default().build(),
            baked_data: data,
        }
    }

    /// Checks if a 3D world position is within the bounding box bounds.
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
