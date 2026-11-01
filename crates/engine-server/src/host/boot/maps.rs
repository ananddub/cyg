use std::path::Path;

use engine_config::BootMaps;
pub(crate) fn boot_oxcl(
    boot: &BootMaps,
    project_dir: &Path,
) -> Option<engine_physics::oxcl::CookedMesh> {
    let console_oxcl = boot.oxcl.clone();
    let oxcl_bytes = match &console_oxcl {
        Some(asset) => {
            tracing::info!(
                "[server] 🗺️  console map '{}' (oxcl v{}) overrides local arena.oxcl",
                asset.name,
                asset.version
            );
            Some(asset.bytes.clone())
        }
        None => std::fs::read(project_dir.join("arena.oxcl")).ok(),
    };
    let arena_mesh: Option<engine_physics::oxcl::CookedMesh> = match oxcl_bytes.as_deref() {
        Some(bytes) => match engine_physics::oxcl::CookedMesh::from_oxcl_bytes(bytes) {
            Ok(mesh) => {
                tracing::info!(
                    "[server] 🗺️  oxcl arena loaded ({} verts, {} indices)",
                    mesh.vertices.len(),
                    mesh.indices.len()
                );
                Some(mesh)
            }
            Err(e) => {
                tracing::error!("[server] failed to parse the oxcl arena: {e}");
                None
            }
        },
        None => None,
    };
    arena_mesh
}
