use glam::Vec2;
use polyanya::{Mesh, MeshError, PolyanyaFile, Polygon, Trimesh, Vertex};

pub struct NavMesh2D {
    pub mesh: Mesh,
}

impl NavMesh2D {

    pub fn new(vertices: Vec<Vertex>, polygons: Vec<Polygon>) -> Result<Self, MeshError> {
        let mesh = Mesh::new(vertices, polygons)?;
        Ok(Self { mesh })
    }

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

    pub fn from_file(path: &str) -> Result<Self, MeshError> {
        let file = PolyanyaFile::from_file(path);
        Self::new(file.vertices, file.polygons)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MeshError> {
        let file = PolyanyaFile::from_bytes(bytes);
        Self::new(file.vertices, file.polygons)
    }

    pub fn find_path(&self, from: (f32, f32), to: (f32, f32)) -> Option<Vec<(f32, f32)>> {
        self.mesh
            .path(Vec2::new(from.0, from.1), Vec2::new(to.0, to.1))
            .map(|p| p.path.into_iter().map(|v| (v.x, v.y)).collect())
    }
}
