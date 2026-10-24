//! # GLTF / GLB Collision Asset Cooker
//!
//! Extracts triangle mesh vertices and index buffers from standard `.glb` or `.gltf`
//! 3D scene files and bakes them into OpenOxide's high-speed flat binary `OXCL` format.
//!
//! Loading an `OXCL` binary at server boot requires zero JSON/GLTF decoding and
//! starts the physics plane in under 1 millisecond.

use std::fs;
use std::path::Path;

// The OXCL binary format lives in engine-physics so both the CLI cooker and
// the server's ECS room backend share one codec.
pub use engine_physics::oxcl::CookedMesh;

/// Reads a `.glb` or `.gltf` model, extracts mesh geometry, and saves as an `OXCL` binary.
pub fn cook_glb(input: &Path, output: &Path) -> Result<CookedMesh, Box<dyn std::error::Error>> {
    let (doc, buffers, _) = gltf::import(input)?;
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|buf| Some(&buffers[buf.index()]));
            if let Some(pos_iter) = reader.read_positions() {
                let base_idx = vertices.len() as u32;
                vertices.extend(pos_iter);

                if let Some(idx_iter) = reader.read_indices() {
                    indices.extend(idx_iter.into_u32().map(|i| i + base_idx));
                }
            }
        }
    }

    let cooked = CookedMesh { vertices, indices };
    let bytes = cooked.to_oxcl_bytes();
    fs::write(output, bytes)?;

    println!(
        "✔ Cooked {} vertices and {} indices into {:?}",
        cooked.vertices.len(),
        cooked.indices.len(),
        output
    );

    Ok(cooked)
}
