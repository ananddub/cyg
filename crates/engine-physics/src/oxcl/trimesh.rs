use crate::rapier::RapierWorld3D;
use rapier3d::prelude as r3d;

use super::CookedMesh;

impl RapierWorld3D {
    pub fn add_oxcl_mesh(&mut self, mesh: &CookedMesh) {
        if mesh.indices.len() < 3 || mesh.vertices.is_empty() {
            return;
        }
        let points: Vec<r3d::Vector> = mesh
            .vertices
            .iter()
            .map(|v| r3d::Vector::new(v[0], v[1], v[2]))
            .collect();
        let indices: Vec<[u32; 3]> = mesh
            .indices
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();

        let Ok(collider) = r3d::ColliderBuilder::trimesh(points, indices) else {
            tracing::warn!("[physics] ⚠️ OXCL mesh has degenerate triangles — arena skipped");
            return;
        };
        let body = r3d::RigidBodyBuilder::fixed().build();
        let handle = self.rigid_body_set.insert(body);
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        self.step();
    }
}
