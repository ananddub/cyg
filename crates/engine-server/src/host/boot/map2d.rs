use std::path::Path;

use engine_config::BootMaps;
pub(crate) fn boot_map2d(
    boot: &BootMaps,
    project_dir: &Path,
) -> Option<engine_physics::MapConfig2D> {
    let map_path = project_dir.join("arena_map.json");
    let arena_2d_path = project_dir.join("arena_2d.json");

    let arena_2d: Option<engine_physics::MapConfig2D> = if let Some(asset) = boot.map2d.as_ref() {
        match serde_json::from_slice::<engine_physics::MapConfig2D>(&asset.bytes) {
            Ok(m) => {
                tracing::info!(
                    "[server] 🗺️  console map '{}' (map2d v{}): {} static colliders ({}x{})",
                    asset.name,
                    asset.version,
                    m.obstacles.len(),
                    m.arena_width,
                    m.arena_height
                );
                Some(m)
            }
            Err(e) => {
                tracing::error!(
                    "[server] failed to parse console map2d '{}': {e}",
                    asset.name
                );
                None
            }
        }
    } else if arena_2d_path.exists() {
        match std::fs::read_to_string(&arena_2d_path) {
            Ok(json_str) => match serde_json::from_str::<engine_physics::MapConfig2D>(&json_str) {
                Ok(m) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena_2d.json with {} static colliders ({}x{})",
                        m.obstacles.len(),
                        m.arena_width,
                        m.arena_height
                    );
                    Some(m)
                }
                Err(e) => {
                    tracing::error!("[server] failed to parse arena_2d.json: {e}");
                    None
                }
            },
            Err(_) => None,
        }
    } else if map_path.exists() {
        match std::fs::read_to_string(&map_path) {
            Ok(json_str) => match serde_json::from_str::<engine_physics::MapConfig2D>(&json_str) {
                Ok(m) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena_map.json as 2D map with {} static colliders ({}x{})",
                        m.obstacles.len(),
                        m.arena_width,
                        m.arena_height
                    );
                    Some(m)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };
    arena_2d
}
