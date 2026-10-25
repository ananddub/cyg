//! # OXCL — OpenOxide CoLlision binary format
//!
//! Flat, parse-free collision mesh format produced by `ox-game cook` from
//! `.glb`/`.gltf` sources. Loading an OXCL arena at server boot is a pure
//! byte read — no JSON/GLTF decoding — and bakes straight into a static
//! Rapier trimesh collider.

use std::io;

/// Magic 4-byte header: "OXCL" (OpenOXide CoLlision)
pub const MAGIC: &[u8; 4] = b"OXCL";
/// Format version
pub const VERSION: u16 = 1;

/// Cooked collision mesh geometry
#[derive(Debug, Clone, PartialEq)]
pub struct CookedMesh {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl CookedMesh {
    /// Serializes the cooked mesh into flat `OXCL` binary bytes.
    /// Header layout (16 bytes):
    /// - [0..4]   Magic: b"OXCL"
    /// - [4..6]   Version: u16 (LE)
    /// - [6..8]   Flags / Reserved: u16 (LE)
    /// - [8..12]  Vertex count: u32 (LE)
    /// - [12..16] Index count: u32 (LE)
    pub fn to_oxcl_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.vertices.len() * 12 + self.indices.len() * 4);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // Reserved flags
        buf.extend_from_slice(&(self.vertices.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.indices.len() as u32).to_le_bytes());

        for v in &self.vertices {
            buf.extend_from_slice(&v[0].to_le_bytes());
            buf.extend_from_slice(&v[1].to_le_bytes());
            buf.extend_from_slice(&v[2].to_le_bytes());
        }

        for idx in &self.indices {
            buf.extend_from_slice(&idx.to_le_bytes());
        }

        buf
    }

    /// Deserializes a cooked mesh from raw `OXCL` binary bytes
    pub fn from_oxcl_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File too small for OXCL header",
            ));
        }

        if &bytes[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid magic header, expected OXCL",
            ));
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported OXCL version: {}", version),
            ));
        }

        let vert_count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        let idx_count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;

        let expected_len = 16 + vert_count * 12 + idx_count * 4;
        if bytes.len() < expected_len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "OXCL payload truncated",
            ));
        }

        let mut offset = 16;
        let mut vertices = Vec::with_capacity(vert_count);
        for _ in 0..vert_count {
            let x = f32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            let y = f32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]);
            let z = f32::from_le_bytes([
                bytes[offset + 8],
                bytes[offset + 9],
                bytes[offset + 10],
                bytes[offset + 11],
            ]);
            offset += 12;
            vertices.push([x, y, z]);
        }

        let mut indices = Vec::with_capacity(idx_count);
        for _ in 0..idx_count {
            let idx = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;
            indices.push(idx);
        }

        Ok(Self { vertices, indices })
    }
}

use crate::rapier::RapierWorld3D;
use rapier3d::prelude as r3d;

impl RapierWorld3D {
    /// Bake a cooked OXCL mesh into this world as one static trimesh collider.
    /// Triangles the server can never walk through; use for arena geometry.
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

        // Degenerate triangles (zero-area) are rejected by parry; an arena
        // with none of them always builds.
        let Ok(collider) = r3d::ColliderBuilder::trimesh(points, indices) else {
            tracing::warn!("[physics] ⚠️ OXCL mesh has degenerate triangles — arena skipped");
            return;
        };
        let body = r3d::RigidBodyBuilder::fixed().build();
        let handle = self.rigid_body_set.insert(body);
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set);
        // One step so the query pipeline indexes the fresh trimesh — without
        // it a pre-tick raycast (bullet spawn at join time) misses entirely.
        self.step();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oxcl_roundtrip_and_trimesh() {
        let mesh = CookedMesh {
            vertices: vec![
                [-50.0, 0.0, -50.0],
                [50.0, 0.0, -50.0],
                [50.0, 0.0, 50.0],
                [-50.0, 0.0, 50.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        };
        let bytes = mesh.to_oxcl_bytes();
        let loaded = CookedMesh::from_oxcl_bytes(&bytes).expect("valid oxcl");
        assert_eq!(loaded, mesh);

        // A ball dropped onto the baked arena must land on it, not fall through.
        let mut world = RapierWorld3D::default();
        world.add_oxcl_mesh(&loaded);
        let body = r3d::RigidBodyBuilder::dynamic()
            .translation(r3d::Vector::new(0.0, 5.0, 0.0))
            .build();
        let handle = world.rigid_body_set.insert(body);
        let col = r3d::ColliderBuilder::ball(0.5).build();
        world
            .collider_set
            .insert_with_parent(col, handle, &mut world.rigid_body_set);
        for _ in 0..120 {
            world.step();
        }
        let y = world.rigid_body_set[handle].translation().y;
        assert!(y < 1.5 && y >= 0.40, "ball must rest on the OXCL arena, got {y}");

        // A ray straight down must hit the baked arena immediately.
        let hit = world.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
        assert!(hit.is_some(), "ray down must hit the OXCL arena");
    }
}

// NOTE: rapier3d 0.36.1's `cast_ray` doc example is broken — a freshly built
// `BroadPhaseBvh` never sees colliders until `set_aabb`/`step` syncs them.
// That is why `RapierWorld3D::raycast_hit` runs `sync_query_tree()` first.
#[test]
fn doc_style_cuboid_raycast_works() {
    // Doc-faithful baseline: cuboid collider, no step, immediate raycast.
    let mut world = RapierWorld3D::default();
    let ground = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
        .translation(r3d::Vector::new(0.0, -0.5, 0.0))
        .build();
    world.collider_set.insert(ground);
    let hit = world.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
    assert!(hit.is_some(), "cuboid baseline raycast");
}

#[test]
fn trimesh_raycast_isolated() {
    let mesh = CookedMesh {
        vertices: vec![[-50.0, 0.0, -50.0], [50.0, 0.0, -50.0], [50.0, 0.0, 50.0]],
        indices: vec![0, 1, 2],
    };
    let mut world = RapierWorld3D::default();
    world.add_oxcl_mesh(&mesh);
    // sync_query_tree path (no step at all beyond add_oxcl_mesh's own).
    let hit = world.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
    assert!(hit.is_some(), "trimesh raycast (lazy sync)");
}

#[test]
fn raycast_minimal_debug() {
    use rapier3d::parry::query::RayCast;
    // 1. Direct local-frame cast on a cuboid: no transform at all.
    let shape = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0).build();
    let ray = r3d::Ray::new(
        r3d::Vector::new(0.0, 10.5, 0.0),
        r3d::Vector::new(0.0, -1.0, 0.0),
    );
    let hit = shape.shape().cast_local_ray(&ray, 100.0, true);
    assert!(hit.is_some(), "direct local cuboid cast, got {hit:?}");

    // 2. Via world position inverse transform.
    let mut world = RapierWorld3D::default();
    let ground = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
        .translation(r3d::Vector::new(0.0, -0.5, 0.0))
        .build();
    world.collider_set.insert(ground);
    let world_ray = r3d::Ray::new(
        r3d::Vector::new(0.0, 10.0, 0.0),
        r3d::Vector::new(0.0, -1.0, 0.0),
    );
    for (h, col) in world.collider_set.iter() {
        let _ = h;
        // parentless collider: use the collider's own position
        let pos = col.position();
        let local = world_ray.transform_by(&pos.inverse());
        let hit = col.shape().cast_local_ray(&local, 100.0, true);
        assert!(hit.is_some(), "position-inverse cuboid cast, got {hit:?}");
    }
}
