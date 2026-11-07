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

impl GameConfig {
    /// Operator patch-interval cap in milliseconds (`patch_rate` Hz → ms).
    /// 0 = uncapped (the guest's `setPatchRate` alone decides).
    pub fn patch_rate_ms(&self) -> i32 {
        if self.patch_rate == 0 {
            0
        } else {
            (1000 / self.patch_rate.max(1)) as i32
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
    /// Operator cap on state-broadcast frequency, in patches per second
    /// (Colyseus `patchRate` parity). 0 = follow the guest's `setPatchRate`
    /// (default: patch every tick). When set, the effective interval is the
    /// SLOWER of this and the guest's rate — the operator can throttle a
    /// room but never force it faster than the guest asked.
    pub patch_rate: u32,
    /// Compile guests with a bounded linear memory (`--maxMemory`) and run
    /// them on wasmtime's pooling allocator (instance slots reused across
    /// room create/dispose). False restores the unbounded linear allocator.
    pub wasm_pooling: bool,
    /// Guest linear-memory cap in MiB when wasm_pooling is on (pooling needs
    /// a declared maximum). Raise for memory-heavy rooms.
    pub wasm_max_memory_mb: u32,
    /// Room tick staggering: split the tick interval into this many phase
    /// slots and give each room its own slot, so hundreds of rooms no longer
    /// all simulate and fan out in one burst at the tick boundary. Per-room
    /// tick rate is unchanged — only the phase shifts. 1 = off (all rooms
    /// tick together, the historical behavior).
    pub tick_stagger: u32,
    pub max_clients: i32,

    pub max_rooms: u32,

    pub max_messages_per_second: u32,

    /// Server-side create options for every room (JSON object) — merged with
    /// the creator's `joinOrCreate` options (client keys win) and handed to
    /// `onCreate(max, options)`. Colyseus's `defineRoom(MyRoom, {…})` merge.
    pub room_options: Option<String>,

    /// Named room variants (Colyseus's `defineRoom` registry). A variant with
    /// `filter_by` keys turns matchmaking into "join the room whose options
    /// match, else create one": `joinOrCreate("duo", { region: "mumbai" })`
    /// lands in the `duo` variant's `region=mumbai` room, not the first empty
    /// seat. Declare as `[rooms.duo]` tables in config.toml.
    pub rooms: std::collections::BTreeMap<String, RoomVariantDef>,

    pub ws_port: u16,

    pub simulate_latency_ms: u32,
    pub simulate_packet_loss: f64,
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
    pub features: FeaturesConfig,
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
            patch_rate: 0,
            wasm_pooling: true,
            wasm_max_memory_mb: 64,
            tick_stagger: 1,
            max_clients: 16,
            max_rooms: 1024,
            max_messages_per_second: 0,
            room_options: None,
            rooms: Default::default(),
            ws_port: 0,
            simulate_latency_ms: 0,
            simulate_packet_loss: 0.0,
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
            features: FeaturesConfig::default(),
        }
    }
}


/// Which per-room physics backend the engine should construct and expose.
/// "none" keeps the guest's `sys_rapier*` imports unfulfilled-by-host: the
/// linker answers with permission-denied and no rapier world is allocated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PhysicsFeature {
    /// Every backend available (today's behavior — backwards compatible).
    #[default]
    Both,
    /// Only the 2D rapier world (`sys_rapier2d_*`); 3D calls are refused.
    D2,
    /// Only the 3D rapier world (`sys_rapier3d_*`); 2D calls are refused.
    D3,
    /// No rapier world at all — pure-logic/turn-based games save the
    /// per-room world allocations entirely.
    None,
}

impl FeaturesConfig {
    pub fn wants_rapier2d(&self) -> bool {
        self.physics.wants_rapier2d()
    }

    pub fn wants_rapier3d(&self) -> bool {
        self.physics.wants_rapier3d()
    }

    pub fn wants_any_physics(&self) -> bool {
        self.physics.wants_any()
    }
}

impl PhysicsFeature {
    pub fn wants_rapier2d(self) -> bool {
        matches!(self, PhysicsFeature::Both | PhysicsFeature::D2)
    }

    pub fn wants_rapier3d(self) -> bool {
        matches!(self, PhysicsFeature::Both | PhysicsFeature::D3)
    }

    pub fn wants_any(self) -> bool {
        self != PhysicsFeature::None
    }

    /// `"both" | "2d" | "3d" | "none"` for TOML/env parsing.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "both" | "all" => Some(PhysicsFeature::Both),
            "2d" => Some(PhysicsFeature::D2),
            "3d" => Some(PhysicsFeature::D3),
            "none" | "off" => Some(PhysicsFeature::None),
            _ => None,
        }
    }
}

/// Config-driven feature loading — `[features]` table. Anything the operator
/// leaves off is never constructed per room, saving its RAM and tick CPU.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FeaturesConfig {
    pub physics: PhysicsFeature,
    /// When false, voice packets are dropped at the door and the voice mesh
    /// never routes (structs stay, allocation cost is negligible).
    pub voice: bool,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            physics: PhysicsFeature::Both,
            voice: true,
        }
    }
}

/// One named room variant — Colyseus's `defineRoom(MyRoom, {…}).filterBy([...])`
/// equivalent. The variant name is the room name the client joins; `filter_by`
/// lists the join options that split players into separate rooms.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoomVariantDef {
    /// Join-option keys that separate rooms ("region" → one room per region).
    #[serde(default)]
    pub filter_by: Vec<String>,
    /// Per-variant capacity override (falls back to the game's max_clients).
    #[serde(default)]
    pub max_clients: Option<i32>,
    /// Per-variant persistence override (hibernate instead of dispose).
    #[serde(default)]
    pub persistent: Option<bool>,
    /// Per-variant create options handed to `onCreate` (client keys win).
    #[serde(default)]
    pub options: Option<String>,
}

impl RoomVariantDef {
    /// The room-key suffix this variant produces for a join: sorted
    /// `key-value` pairs over the declared `filter_by` keys present in the
    /// options — deterministic, so equal filters land in one room.
    pub fn room_key_suffix(&self, options: &str) -> String {
        if self.filter_by.is_empty() {
            return String::new();
        }
        let value = serde_json::from_str::<serde_json::Value>(options).ok();
        let object = value.as_ref().and_then(|value| value.as_object());
        let mut pairs: Vec<String> = self
            .filter_by
            .iter()
            .filter_map(|key| {
                object
                    .and_then(|object| object.get(key.as_str()))
                    .map(|value| format!("{key}-{}", flat_value(value)))
            })
            .collect();
        pairs.sort();
        if pairs.is_empty() {
            return String::new();
        }
        format!("__{}", pairs.join("_"))
    }
}

/// Scalars flattened for the room key: strings verbatim (sanitized upstream),
/// numbers/bools as text, missing as `-`.
fn flat_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        _ => "-".to_string(),
    }
}
