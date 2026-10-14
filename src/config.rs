//! # ox-game configuration
//!
//! Flat `key: value` YAML — no external parser needed. Every key is optional
//! unless noted; `ox-game init` writes a ready-to-edit template.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// AssemblyScript — source compiled with `asc` (npm assemblyscript).
    AssemblyScript,
    /// Lua — source executed via Lua guest VM (Lua 5.4 / LuaJIT).
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

/// Full server + build configuration (from config.yml).
#[derive(Debug, Clone)]
pub struct GameConfig {
    pub name: String,
    pub runtime: Runtime,
    /// Room source file, relative to the project directory.
    pub source: PathBuf,
    /// Compiled WASM output, relative to the project directory.
    pub out: PathBuf,
    /// Directory containing the SDK bundles (colyseus.js + schema build).
    pub sdk_dir: PathBuf,
    /// Website HTML, relative to the project directory (optional —
    /// falls back to the built-in template when missing).
    pub website: PathBuf,
    pub port: u16,
    pub web_port: u16,
    pub tick_rate: u32,
    pub max_clients: i32,
    pub width: f64,
    pub height: f64,
    pub speed: f64,
    /// ABI version mandatory contract (default: 1)
    pub abi_version: u32,
    /// Capability-based permissions for host function access
    pub permissions: engine_wasm::AbiPermissions,
    /// ICE server list handed to browsers for WebRTC voice, as raw JSON text
    /// (`[{ "urls": [...] }, ...]`). The server never parses it — it is relayed
    /// verbatim inside a `Config` signal frame, so a malformed value is the
    /// browser's problem, not a startup failure. Empty means P2P over LAN only.
    pub voice_ice_servers: String,
    /// Network interest management (Phase E): per-client AOI culling and
    /// tick-skip priority for distant entities.
    pub aoi: AoiConfig,
    /// Server-side rollback of late inputs (Phase F): rewind the guest to the
    /// snapshot before the input's position in its stream, replay the journaled
    /// inputs, and resimulate to the live tick.
    pub rollback: RollbackConfig,
}

/// `[rollback]` block: GGPO-style late-input rewind on the authoritative host.
#[derive(Debug, Clone, Copy)]
pub struct RollbackConfig {
    /// Master switch. Off keeps the legacy newest-wins drop for stale seqs.
    pub enabled: bool,
    /// How far back (in ticks) a late input may rewind the simulation. Inputs
    /// older than this are dropped — replay cost grows linearly with the gap.
    pub max_replay_ticks: u64,
    /// Recent applied inputs kept per room for replay ordering.
    pub journal_size: usize,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_replay_ticks: 32,
            journal_size: 256,
        }
    }
}

/// `[aoi]` block: cull and prioritize the per-tick delta per viewer instead of
/// multicasting one identical buffer to the whole room.
#[derive(Debug, Clone, Copy)]
pub struct AoiConfig {
    /// Master switch. Off keeps the legacy whole-room broadcast.
    pub enabled: bool,
    /// Entities within this 3D distance of a viewer are sent **every tick**.
    pub near_radius: f64,
    /// Entities inside `far_radius` but outside `near_radius` are sent every
    /// `skip_ticks`-th tick (clients dead-reckon between them).
    pub far_radius: f64,
    /// Priority cadence for far entities: 4 = every fourth tick.
    pub skip_ticks: u64,
    /// Bullet/ projectile cull radius around each viewer.
    pub bullet_radius: f64,
}

impl Default for AoiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            near_radius: 180.0,
            far_radius: 420.0,
            skip_ticks: 4,
            bullet_radius: 260.0,
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            name: "my-game".into(),
            runtime: Runtime::AssemblyScript,
            source: "room.ts".into(),
            out: "room_logic.wasm".into(),
            sdk_dir: "/mnt/SSD/Devloper/pannel/colyseus".into(),
            website: "website.html".into(),
            port: 7777,
            web_port: 8090,
            tick_rate: 20,
            max_clients: 16,
            width: 800.0,
            height: 500.0,
            speed: 240.0,
            abi_version: engine_wasm::CURRENT_ABI_VERSION,
            permissions: engine_wasm::AbiPermissions::default(),
            voice_ice_servers: "[]".into(),
            aoi: AoiConfig::default(),
            rollback: RollbackConfig::default(),
        }
    }
}

/// Parse a TOML or flat `key: value` config.
pub fn parse_config(text: &str) -> GameConfig {
    let mut cfg = GameConfig::default();

    // 1. Try parsing as standard TOML first
    if let Ok(value) = text.parse::<toml::Value>() {
        if let Some(table) = value.as_table() {
            if let Some(v) = table.get("name").and_then(|v| v.as_str()) {
                cfg.name = v.to_string();
            }
            if let Some(v) = table.get("runtime").and_then(|v| v.as_str()) {
                if let Some(r) = Runtime::parse(v) {
                    cfg.runtime = r;
                }
            }
            if let Some(v) = table.get("source").and_then(|v| v.as_str()) {
                cfg.source = v.into();
            }
            if let Some(v) = table.get("out").and_then(|v| v.as_str()) {
                cfg.out = v.into();
            }
            if let Some(v) = table.get("sdk_dir").and_then(|v| v.as_str()) {
                cfg.sdk_dir = v.into();
            }
            if let Some(v) = table.get("website").and_then(|v| v.as_str()) {
                cfg.website = v.into();
            }
            if let Some(v) = table.get("port").and_then(|v| v.as_integer()) {
                cfg.port = v as u16;
            }
            if let Some(v) = table.get("web_port").and_then(|v| v.as_integer()) {
                cfg.web_port = v as u16;
            }
            if let Some(v) = table.get("tick_rate").and_then(|v| v.as_integer()) {
                cfg.tick_rate = v as u32;
            }
            if let Some(v) = table.get("max_clients").and_then(|v| v.as_integer()) {
                cfg.max_clients = v as i32;
            }
            if let Some(v) = table.get("width").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) {
                cfg.width = v;
            }
            if let Some(v) = table.get("height").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) {
                cfg.height = v;
            }
            if let Some(v) = table.get("speed").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) {
                cfg.speed = v;
            }
            if let Some(v) = table.get("abi_version").and_then(|v| v.as_integer()) {
                cfg.abi_version = v as u32;
            }
            if let Some(v) = table.get("allow_physics").and_then(|v| v.as_bool()) {
                cfg.permissions.physics = v;
            }
            if let Some(v) = table.get("allow_lag_compensation").and_then(|v| v.as_bool()) {
                cfg.permissions.lag_compensation = v;
            }
            if let Some(v) = table.get("allow_logging").and_then(|v| v.as_bool()) {
                cfg.permissions.logging = v;
            }
            if let Some(v) = table.get("allow_storage").and_then(|v| v.as_bool()) {
                cfg.permissions.storage = v;
            }
            if let Some(v) = table.get("voice_ice_servers") {
                cfg.voice_ice_servers = v.to_string();
            }
            if let Some(a) = table.get("aoi").and_then(|v| v.as_table()) {
                cfg.aoi.enabled = a.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                if let Some(v) = a.get("near_radius").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) {
                    cfg.aoi.near_radius = v;
                }
                if let Some(v) = a.get("far_radius").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) {
                    cfg.aoi.far_radius = v;
                }
                if let Some(v) = a.get("skip_ticks").and_then(|v| v.as_integer()) {
                    cfg.aoi.skip_ticks = (v.max(1)) as u64;
                }
                if let Some(v) = a.get("bullet_radius").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))) {
                    cfg.aoi.bullet_radius = v;
                }
            }
            if let Some(r) = table.get("rollback").and_then(|v| v.as_table()) {
                cfg.rollback.enabled = r.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                if let Some(v) = r.get("max_replay_ticks").and_then(|v| v.as_integer()) {
                    cfg.rollback.max_replay_ticks = v.max(1) as u64;
                }
                if let Some(v) = r.get("journal_size").and_then(|v| v.as_integer()) {
                    cfg.rollback.journal_size = (v.max(8)) as usize;
                }
            }
            return cfg;
        }
    }

    // 2. Fallback parser for key = value or key: value
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let (key, value) = if let Some((k, v)) = line.split_once('=') {
            (k, v)
        } else if let Some((k, v)) = line.split_once(':') {
            (k, v)
        } else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        match key {
            "name" => cfg.name = value.to_string(),
            "runtime" => {
                if let Some(r) = Runtime::parse(value) {
                    cfg.runtime = r;
                }
            }
            "source" => cfg.source = value.into(),
            "out" => cfg.out = value.into(),
            "sdk_dir" => cfg.sdk_dir = value.into(),
            "website" => cfg.website = value.into(),
            "port" => cfg.port = value.parse().unwrap_or(cfg.port),
            "web_port" => cfg.web_port = value.parse().unwrap_or(cfg.web_port),
            "tick_rate" => cfg.tick_rate = value.parse().unwrap_or(cfg.tick_rate),
            "max_clients" => cfg.max_clients = value.parse().unwrap_or(cfg.max_clients),
            "width" => cfg.width = value.parse().unwrap_or(cfg.width),
            "height" => cfg.height = value.parse().unwrap_or(cfg.height),
            "speed" => cfg.speed = value.parse().unwrap_or(cfg.speed),
            "abi_version" => cfg.abi_version = value.parse().unwrap_or(cfg.abi_version),
            "allow_physics" => cfg.permissions.physics = value.parse().unwrap_or(cfg.permissions.physics),
            "allow_lag_compensation" => cfg.permissions.lag_compensation = value.parse().unwrap_or(cfg.permissions.lag_compensation),
            "allow_logging" => cfg.permissions.logging = value.parse().unwrap_or(cfg.permissions.logging),
            "allow_storage" => cfg.permissions.storage = value.parse().unwrap_or(cfg.permissions.storage),
            "voice_ice_servers" => cfg.voice_ice_servers = value.to_string(),
            _ => {}
        }
    }
    cfg
}

/// Load config.toml (or fallback config.yml) from a directory (or the file itself).
pub fn load_config(path: &Path) -> std::io::Result<GameConfig> {
    let file = if path.is_dir() {
        if path.join("config.toml").exists() {
            path.join("config.toml")
        } else {
            path.join("config.yml")
        }
    } else {
        path.to_path_buf()
    };
    let text = std::fs::read_to_string(&file)?;
    Ok(parse_config(&text))
}

/// The config.toml template `ox-game init` writes.
pub const CONFIG_TEMPLATE: &str = r#"# ox-game configuration (TOML)
name = "my-game"

# Runtime used to compile your room source into WebAssembly.
# Supported: assemblyscript, lua, cpp, rust, csharp
runtime = "assemblyscript"

# Your room logic source + compiled output (relative to this folder).
source = "room.ts"
out = "room_logic.wasm"

# Where the @colyseus/sdk + @colyseus/schema bundles live.
sdk_dir = "/mnt/SSD/Devloper/pannel/colyseus"

# Your website (canvas page). Served at http://localhost:<web_port>/
website = "website.html"

port = 7777
web_port = 8090
tick_rate = 128
max_clients = 16

# Playfield
width = 800.0
height = 500.0
speed = 240.0

# OpenOxide ABI Contract & Permissions
abi_version = 1
allow_physics = true
allow_lag_compensation = true
allow_logging = true
allow_storage = true
"#;

/// Starter room source `ox-game init` writes — the same contract as the
/// engine expects: on_create / on_join / on_leave / on_input / update.
pub const ROOM_TEMPLATE: &str = r#"// Room movement logic — compiled to WebAssembly, run by the Rust host
// through the engine-wasm crate (server-authoritative simulation).
//
// Host -> guest ABI (all numeric so the wrapper stays trivial):
//   on_create(max)            room created
//   on_join(id, x, y)         player spawned
//   on_leave(id)              player removed
//   on_input(id, flags)       held-key bitmask (W=1 S=2 A=4 D=8)
//   update(dt, w, h, speed)   advance the simulation one tick
//   get_count/get_id/get_x/get_y   host reads the world back

class Player {
    id: i64 = 0;
    x: f64 = 0;
    y: f64 = 0;
    flags: u32 = 0;
}

var players: Player[] = new Array<Player>();
var max_players: i32 = 16;

export function on_create(max: i32): void {
    max_players = max;
    players.length = 0;
}

export function is_full(count: i32): bool {
    return count >= max_players;
}

export function on_join(id: i64, x: f64, y: f64): void {
    let p = new Player();
    p.id = id;
    p.x = x;
    p.y = y;
    players.push(p);
}

export function on_leave(id: i64): void {
    for (let i = 0; i < players.length; i++) {
        if (unchecked(players[i]).id == id) {
            players.splice(i, 1);
            return;
        }
    }
}

export function on_input(id: i64, flags: u32): void {
    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        if (p.id == id) {
            p.flags = flags;
            return;
        }
    }
}

export function update(dt: f64, w: f64, h: f64, speed: f64): void {
    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        let f = p.flags;
        if (f & 1) p.y -= speed * dt; // W
        if (f & 2) p.y += speed * dt; // S
        if (f & 4) p.x -= speed * dt; // A
        if (f & 8) p.x += speed * dt; // D

        if (p.x < 14) p.x = 14;
        if (p.x > w - 14) p.x = w - 14;
        if (p.y < 14) p.y = 14;
        if (p.y > h - 14) p.y = h - 14;
    }
}

export function get_count(): i32 {
    return players.length;
}

export function get_id(i: i32): i64 {
    return unchecked(players[i]).id;
}

export function get_x(i: i32): f64 {
    return unchecked(players[i]).x;
}

export function get_y(i: i32): f64 {
    return unchecked(players[i]).y;
}
"#;
