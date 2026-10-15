//! Starter templates written by `ox-game init`. The comments inside the strings are
//! user-facing config/room docs, not code commentary.

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
