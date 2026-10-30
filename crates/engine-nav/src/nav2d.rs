use glam::Vec2;
use polyanya::{Mesh, MeshError, PolyanyaFile, Polygon, Trimesh, Vertex};

/// 2D Navigation Mesh powered by Polyanya (Any-Angle Pathfinding)
pub struct NavMesh2D {
    pub mesh: Mesh,
}

impl NavMesh2D {
    /// Creates a 2D navmesh from vertices and polygonal faces.
    pub fn new(vertices: Vec<Vertex>, polygons: Vec<Polygon>) -> Result<Self, MeshError> {
        let mesh = Mesh::new(vertices, polygons)?;
        Ok(Self { mesh })
    }

    /// Creates a 2D navmesh from simple 2D triangle mesh (vertices + triangle index tuples).
    pub fn from_trimesh(
        vertices: Vec<(f32, f32)>,
        triangles: Vec<[usize; 3]>,
    ) -> Result<Self, MeshError> {
        let v: Vec<Vec2> = vertices.into_iter().map(|(x, y)| Vec2::new(x, y)).collect();
        let trimesh = Trimesh {
            vertices: v,
            triangles,
        };
        let mesh: Mesh = trimesh.try_into()?;
        Ok(Self { mesh })
    }

    /// Loads a navmesh from a `.mesh` file path.
    pub fn from_file(path: &str) -> Result<Self, MeshError> {
        let file = PolyanyaFile::from_file(path);
        Self::new(file.vertices, file.polygons)
    }

    /// Loads a navmesh from raw bytes in the standard Polyanya format.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MeshError> {
        let file = PolyanyaFile::from_bytes(bytes);
        Self::new(file.vertices, file.polygons)
    }

    /// Finds an optimal any-angle path between two 2D points.
    /// Returns Some(waypoints) or None if unreachable.
    pub fn find_path(&self, from: (f32, f32), to: (f32, f32)) -> Option<Vec<(f32, f32)>> {
        self.mesh
            .path(Vec2::new(from.0, from.1), Vec2::new(to.0, to.1))
            .map(|p| p.path.into_iter().map(|v| (v.x, v.y)).collect())
    }
}
