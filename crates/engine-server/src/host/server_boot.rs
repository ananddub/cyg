use std::path::Path;
use std::time::Instant;

use engine_physics::PhysicsWorld3D;
use engine_wasm::WasmRuntime;

use crate::rooms::{
    GameServer,
    RoomHostData,
};

use crate::host::*;
use engine_config::{BootMaps, BootModule, GameConfig};
use engine_wasm::WasmPreInstance;
pub(crate) struct PreSelection {
    pub(crate) pre: Option<WasmPreInstance<RoomHostData>>,
    pub(crate) console_active: bool,
    pub(crate) wasm_available: bool,
}

pub(crate) fn select_pre_module(
    runtime: &WasmRuntime,
    boot_module: Option<&BootModule>,
    wasm_path: &Path,
    cfg: &GameConfig,
) -> PreSelection {
    tracing::info!(
        "[server] ABI contract: host v{} (manifest requested: v{})",
        engine_wasm::CURRENT_ABI_VERSION,
        cfg.abi_version
    );
    if cfg.abi_version > engine_wasm::CURRENT_ABI_VERSION {
        tracing::warn!(
            "[server] ⚠️  Warning: manifest requests ABI version {}, but server only supports up to v{}",
            cfg.abi_version,
            engine_wasm::CURRENT_ABI_VERSION
        );
    }
    let _wasm_load_start = Instant::now();

    let wasm_load_start = Instant::now();
    let mut console_active = false;
    let pre = match boot_module.as_ref() {
        Some(module) => {
            match prepare_console_module(
                &runtime,
                &module.sha256,
                &module.bytes,
                cfg.permissions,
                &cfg.features,
            ) {
                Ok(pre) => {
                    console_active = true;
                    tracing::info!(
                        "[server] 📦 room module v{} from the console loaded in {:.2?} ({} bytes)",
                        module.version,
                        wasm_load_start.elapsed(),
                        module.bytes.len()
                    );
                    Some(pre)
                }
                Err(e) => {
                    tracing::error!(
                        "[server] ⚠️ console room module v{} rejected: {e} — using {}",
                        module.version,
                        wasm_path.display()
                    );
                    prepare_file_module(&runtime, wasm_path, cfg.permissions, &cfg.features)
                }
            }
        }
        None => prepare_file_module(&runtime, wasm_path, cfg.permissions, &cfg.features),
    };
    let wasm_available = pre.is_some();
    if wasm_available && !console_active {
        let is_aot = wasm_path.with_extension("cwasm").exists();
        tracing::info!(
            "[server] ⚡ room module loaded in {:.2?} (mode: {})",
            wasm_load_start.elapsed(),
            if is_aot {
                "AOT precompiled (.cwasm)"
            } else {
                "JIT compiled (.wasm)"
            }
        );
    } else if !wasm_available {
        tracing::info!(
            "[server] 🧩 no room module — running Unity-relay rooms only (WASM rooms disabled)"
        );
    }

    PreSelection {
        pre,
        console_active,
        wasm_available,
    }
}

pub(crate) fn assemble_server(
    pre: Option<WasmPreInstance<RoomHostData>>,
    cfg: &GameConfig,
    physics: PhysicsWorld3D,
    arena_2d: Option<engine_physics::MapConfig2D>,
    boot: &BootMaps,
) -> GameServer {
    let mut server = GameServer::new_with_features(
        pre,
        cfg.max_clients,
        physics,
        cfg.permissions,
        cfg.features.clone(),
    );
    server.arena_2d = arena_2d;
    server.input_coalesce = cfg.input_coalesce;
    if let Some(asset) = boot.navmesh.as_ref() {
        match engine_nav::nav2d::NavMesh2D::from_bytes(&asset.bytes) {
            Ok(mesh) => {
                tracing::info!(
                    "[server] 🗺️  console navmesh '{}' (v{}) loaded",
                    asset.name,
                    asset.version
                );
                server.navmesh = Some(std::sync::Arc::new(mesh));
            }
            Err(e) => {
                tracing::error!(
                    "[server] failed to parse console navmesh '{}': {e}",
                    asset.name
                );
            }
        }
    }
    server
}
