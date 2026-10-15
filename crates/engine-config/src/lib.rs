//! # ox-game configuration
//!
//! Flat `key: value` YAML — no external parser needed. Every key is optional
//! unless noted; `ox-game init` writes a ready-to-edit template.

use std::path::{Path, PathBuf};

/// The HMAC key both planes fall back to when `OX_SEAT_SECRET` is unset.
///
/// Single source of truth on purpose: the control plane mints a seat and the sim
/// plane verifies the same string over UDP, so two copies of this literal are
/// two chances for a deployed server to reject every one of its own players.
/// It is a *development* key and must be overridden for anything reachable.
pub const DEV_SEAT_SECRET: &str = "ox-dev-seat-secret";

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
    /// Tokio control plane (Phase 1): Axum HTTP, matchmake, WS signaling,
    /// Agones lifecycle.
    pub control: ControlConfig,
}

/// `[control]` block: the Tokio/Axum control plane. Everything here is
/// browser-facing HTTP; the simulation thread never reads it.
#[derive(Debug, Clone)]
pub struct ControlConfig {
    /// Master switch. Off means this plane never binds a port and never mints a
    /// seat token, so a project can opt out without losing the website.
    pub enabled: bool,
    /// Bind address for the control plane. Defaults to `0.0.0.0`.
    pub bind_addr: String,
    /// Control-plane HTTP port. The game WebSocket transport keeps `port` and
    /// the website port `web_port`, so this is its own port by default; `0`
    /// asks the kernel for one (tests). Setting it **equal to `web_port`** is
    /// the cutover: `src/server.rs` then releases the website port to Axum and
    /// stops installing the legacy in-transport static hook.
    pub api_port: u16,
    /// Tokio worker threads for the control plane. Crypto and HTTP do not
    /// belong on the sim core, so this stays small by default.
    pub workers: usize,
    /// How long a minted seat token stays usable.
    pub seat_ttl_secs: u64,
    /// HMAC key for seat tokens. `OX_SEAT_SECRET` wins over this value so a
    /// deployed server never needs its secret in a file.
    pub seat_secret: String,
    /// Sustained matchmake requests accepted per client IP, per second.
    pub rate_limit_rps: u32,
    /// Headroom above `rate_limit_rps` for a burst (a page reload fires several
    /// matchmake calls at once).
    pub rate_limit_burst: u32,
    /// `[control.agones]` — Agones sidecar lifecycle.
    pub agones: AgonesConfig,
}

/// `[control.agones]`: the Kubernetes Dedicated Game Server sidecar client.
#[derive(Debug, Clone)]
pub struct AgonesConfig {
    /// Off by default: nothing happens unless the pod really has a sidecar.
    pub enabled: bool,
    /// Sidecar REST port (`AGONES_SDK_HTTP_PORT` when set by the pod).
    pub http_port: u16,
    /// Interval between `sdk.v1.SDK/Health` pings. Agones marks the pod
    /// unhealthy after three misses, so this must stay well under 10s.
    pub health_interval_ms: u64,
    /// Address of the sidecar. Loopback inside the pod.
    pub host: String,
}

impl Default for AgonesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            http_port: 9358,
            health_interval_ms: 2000,
            host: "127.0.0.1".into(),
        }
    }
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_addr: "0.0.0.0".into(),
            api_port: 8124,
            workers: 2,
            seat_ttl_secs: 300,
            seat_secret: DEV_SEAT_SECRET.into(),
            rate_limit_rps: 20,
            rate_limit_burst: 40,
            agones: AgonesConfig::default(),
        }
    }
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
            control: ControlConfig::default(),
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
            if let Some(c) = table.get("control").and_then(|v| v.as_table()) {
                let num = |k: &str| c.get(k).and_then(|v| v.as_integer()).map(|v| v.max(0) as u64);
                if let Some(v) = c.get("enabled").and_then(|v| v.as_bool()) {
                    cfg.control.enabled = v;
                }
                if let Some(v) = c.get("bind_addr").and_then(|v| v.as_str()) {
                    cfg.control.bind_addr = v.to_string();
                }
                if let Some(v) = num("api_port") {
                    cfg.control.api_port = v as u16;
                }
                if let Some(v) = num("workers").filter(|v| *v > 0) {
                    cfg.control.workers = v as usize;
                }
                if let Some(v) = num("seat_ttl_secs").filter(|v| *v > 0) {
                    cfg.control.seat_ttl_secs = v;
                }
                if let Some(v) = c.get("seat_secret").and_then(|v| v.as_str()) {
                    cfg.control.seat_secret = v.to_string();
                }
                if let Some(v) = num("rate_limit_rps").filter(|v| *v > 0) {
                    cfg.control.rate_limit_rps = v as u32;
                }
                if let Some(v) = num("rate_limit_burst").filter(|v| *v > 0) {
                    cfg.control.rate_limit_burst = v as u32;
                }
                if let Some(a) = c.get("agones").and_then(|v| v.as_table()) {
                    if let Some(v) = a.get("enabled").and_then(|v| v.as_bool()) {
                        cfg.control.agones.enabled = v;
                    }
                    if let Some(v) = a
                        .get("http_port")
                        .and_then(|v| v.as_integer())
                        .filter(|v| *v > 0)
                    {
                        cfg.control.agones.http_port = v as u16;
                    }
                    if let Some(v) = a
                        .get("health_interval_ms")
                        .and_then(|v| v.as_integer())
                        .filter(|v| *v >= 200)
                    {
                        cfg.control.agones.health_interval_ms = v as u64;
                    }
                    if let Some(v) = a.get("host").and_then(|v| v.as_str()) {
                        cfg.control.agones.host = v.to_string();
                    }
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
            "control_enabled" => cfg.control.enabled = value.parse().unwrap_or(cfg.control.enabled),
            "api_port" => cfg.control.api_port = value.parse().unwrap_or(cfg.control.api_port),
            "control_bind_addr" => cfg.control.bind_addr = value.to_string(),
            "control_workers" => cfg.control.workers = value.parse().unwrap_or(cfg.control.workers),
            "seat_ttl_secs" => cfg.control.seat_ttl_secs = value.parse().unwrap_or(cfg.control.seat_ttl_secs),
            "seat_secret" => cfg.control.seat_secret = value.to_string(),
            "rate_limit_rps" => cfg.control.rate_limit_rps = value.parse().unwrap_or(cfg.control.rate_limit_rps),
            "rate_limit_burst" => cfg.control.rate_limit_burst = value.parse().unwrap_or(cfg.control.rate_limit_burst),
            "agones_enabled" => cfg.control.agones.enabled = value.parse().unwrap_or(cfg.control.agones.enabled),
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
    let mut cfg = parse_config(&text);
    // Deployment secrets never belong in a file that ships with the project.
    // `OX_SEAT_SECRET` is also what the sim plane reads, so both planes are
    // guaranteed to agree on the key without a config handshake.
    if let Ok(secret) = std::env::var("OX_SEAT_SECRET") {
        if !secret.is_empty() {
            cfg.control.seat_secret = secret;
        }
    }
    if std::env::var("OX_AGONES").is_ok_and(|v| v == "1") {
        cfg.control.agones.enabled = true;
    }
    Ok(cfg)
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

# Control plane (Tokio/Axum): matchmake, WS signaling, /status, statics.
# `api_port` is its own port so the game WebSocket (`port`) and the website
# (`web_port`) keep working unchanged. Set `api_port = web_port` to hand the
# website port over to Axum entirely.
[control]
enabled = true
bind_addr = "0.0.0.0"
api_port = 8124
workers = 2
seat_ttl_secs = 300
rate_limit_rps = 20
rate_limit_burst = 40
# seat_secret = "change-me"   # prefer the OX_SEAT_SECRET environment variable

# Agones sidecar lifecycle: Ready, Allocated and the Health ping loop.
[control.agones]
enabled = false
http_port = 9358
health_interval_ms = 2000
host = "127.0.0.1"
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
