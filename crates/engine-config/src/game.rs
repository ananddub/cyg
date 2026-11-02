use std::path::PathBuf;

use crate::control::ControlConfig;
use crate::gameplay::GameplayConfig;
use crate::lifecycle::LifecycleConfig;
use crate::netcode::{AoiConfig, RollbackConfig};
use crate::watchdog::WatchdogConfig;

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

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub name: String,

    pub game_id: Option<String>,

    pub game_key: Option<String>,
    pub runtime: Runtime,

    pub source: PathBuf,

    pub out: PathBuf,

    pub sdk_dir: PathBuf,

    pub website: PathBuf,
    pub port: u16,

    pub quic_port: u16,
    pub web_port: u16,
    pub tick_rate: u32,
    pub max_clients: i32,

    pub max_rooms: u32,

    pub max_messages_per_second: u32,

    pub ws_port: u16,

    pub simulate_latency_ms: u32,
    pub width: f64,
    pub height: f64,
    pub speed: f64,
    pub abi_version: u32,
    pub permissions: engine_wasm::AbiPermissions,

    pub voice_ice_servers: String,
    pub aoi: AoiConfig,
    pub rollback: RollbackConfig,
    pub lifecycle: LifecycleConfig,
    pub watchdog: WatchdogConfig,

    pub gameplay: GameplayConfig,
    pub control: ControlConfig,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            name: "my-game".into(),
            game_id: None,
            game_key: None,
            runtime: Runtime::AssemblyScript,
            source: "room.ts".into(),
            out: "room_logic.wasm".into(),
            sdk_dir: "".into(),
            website: PathBuf::new(),
            port: 7777,
            quic_port: 0,
            web_port: 8090,
            tick_rate: 30,
            max_clients: 16,
            max_rooms: 1024,
            max_messages_per_second: 0,
            ws_port: 0,
            simulate_latency_ms: 0,
            width: 800.0,
            height: 500.0,
            speed: 240.0,
            abi_version: engine_wasm::CURRENT_ABI_VERSION,
            permissions: engine_wasm::AbiPermissions::default(),
            voice_ice_servers: "[]".into(),
            aoi: AoiConfig::default(),
            rollback: RollbackConfig::default(),
            lifecycle: LifecycleConfig::default(),
            watchdog: WatchdogConfig::default(),
            gameplay: GameplayConfig::default(),
            control: ControlConfig::default(),
        }
    }
}
