use std::path::Path;

use engine_physics::PhysicsWorld3D;

use engine_config::BootMaps;
pub(crate) fn boot_map3d(
    boot: &BootMaps,
    project_dir: &Path,
    arena_2d: &Option<engine_physics::MapConfig2D>,
) -> PhysicsWorld3D {
    let map_path = project_dir.join("arena_map.json");

    let physics = if let Some(asset) = boot.map3d.as_ref() {
        match PhysicsWorld3D::from_json(std::str::from_utf8(&asset.bytes).unwrap_or_default()) {
            Ok(w) => {
                tracing::info!(
                    "[server] 🗺️  console map '{}' (map3d v{}): {} static colliders override arena_map.json",
                    asset.name,
                    asset.version,
                    w.obstacles.len()
                );
                w
            }
            Err(e) => {
                tracing::error!(
                    "[server] failed to parse console map3d '{}': {e}, using default world",
                    asset.name
                );
                PhysicsWorld3D::default()
            }
        }
    } else if map_path.exists() && arena_2d.is_none() {
        match std::fs::read_to_string(&map_path) {
            Ok(json_str) => match PhysicsWorld3D::from_json(&json_str) {
                Ok(w) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena_map.json with {} static colliders",
                        w.obstacles.len()
                    );
                    w
                }
                Err(e) => {
                    tracing::error!(
                        "[server] failed to parse arena_map.json: {e}, using default world"
                    );
                    PhysicsWorld3D::default()
                }
            },
            Err(_) => PhysicsWorld3D::default(),
        }
    } else {
        PhysicsWorld3D::default()
    };

    physics
}
