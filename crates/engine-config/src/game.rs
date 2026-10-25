use std::path::PathBuf;

use crate::control::ControlConfig;
use crate::netcode::{AoiConfig, RollbackConfig};

/// HMAC key used when `OX_SEAT_SECRET` is unset. Development key — anything
/// reachable must override it. Shared by both planes so mint and verify can
/// never disagree.
pub const DEV_SEAT_SECRET: &str = "ox-dev-seat-secret";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    AssemblyScript,
    Lua,
}

impl Runtime {
    pub fn parse(s: &str) -> Option<Runtime> {
        match s.trim().to_ascii_lowercase().as_str() {
            "assemblyscript" | "as" => Some(Runtime::AssemblyScript),
            "lua" => Some(Runtime::Lua),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Runtime::AssemblyScript => "assemblyscript",
            Runtime::Lua => "lua",
        }
    }
}

/// Full server + build configuration, one project = one of these.
#[derive(Debug, Clone)]
pub struct GameConfig {
    pub name: String,
    pub runtime: Runtime,
    /// Room source, relative to the project directory.
    pub source: PathBuf,
    /// Compiled WASM output, relative to the project directory.
    pub out: PathBuf,
    /// Directory containing the SDK bundles (colyseus.js + schema build).
    pub sdk_dir: PathBuf,
    /// Website HTML; falls back to the built-in template when missing.
    pub website: PathBuf,
    pub port: u16,
    pub web_port: u16,
    pub tick_rate: u32,
    pub max_clients: i32,
    /// Hard ceiling on concurrently live rooms: a join that would create a new
    /// room past this is rejected instead of allocating one. `0` = unlimited.
    pub max_rooms: u32,
    pub width: f64,
    pub height: f64,
    pub speed: f64,
    pub abi_version: u32,
    pub permissions: engine_wasm::AbiPermissions,
    /// Raw JSON relayed verbatim to browsers inside a `Config` signal frame;
    /// empty means P2P over LAN only.
    pub voice_ice_servers: String,
    pub aoi: AoiConfig,
    pub rollback: RollbackConfig,
    pub control: ControlConfig,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            name: "my-game".into(),
            runtime: Runtime::AssemblyScript,
            source: "room.ts".into(),
            out: "room_logic.wasm".into(),
            sdk_dir: "".into(),
            website: "website.html".into(),
            port: 7777,
            web_port: 8090,
            tick_rate: 30,
            max_clients: 16,
            max_rooms: 1024,
            width: 800.0,
            height: 500.0,
            speed: 240.0,
            abi_version: engine_wasm::CURRENT_ABI_VERSION,
            permissions: engine_wasm::AbiPermissions::default(),
            voice_ice_servers: "[]".into(),
            aoi: AoiConfig::default(),
            rollback: RollbackConfig::default(),
            control: ControlConfig::default(),
        }
    }
}
