//! Starter templates written by `ox-game init`. The comments inside the strings are
//! user-facing config/room docs, not code commentary.

/// The config.toml template `ox-game init` writes.
pub const CONFIG_TEMPLATE: &str = r#"# ox-game configuration (TOML)
name = "my-game"

# Runtime used to compile your room source into WebAssembly.
# Supported: assemblyscript, lua, cpp, rust, csharp
runtime = "assemblyscript"

# Your room logic source + compiled output (relative to this folder).
source = "src/index.ts"
out = "room_logic.wasm"

# Legacy Colyseus bundle directory. New projects should use the bundled
# CYG web SDK (`cyg.js`) and can leave this empty.
sdk_dir = ""

port = 7777
web_port = 8090
# 30 Hz is the easy-mode default; use 20 for slower/casual rooms or 60/128 for
# competitive/rollback games.
tick_rate = 30
max_clients = 16
max_rooms = 1024

# Playfield
width = 800.0
height = 500.0
speed = 240.0

# CYG ABI Contract & Permissions
abi_version = 1
allow_physics = true
allow_lag_compensation = true
allow_logging = true
allow_storage = true
allow_messaging = true

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

# BaaS-compatible account plane (/v2/account/*). Defaults match the own
# config.yaml, so an unmodified BaaS SDK works unchanged.
# Leave database_url empty (or unset DATABASE_URL) to keep the game running with
# no accounts database; /v2 then answers 503.
[control.session]
database_url = ""
max_db_connections = 10
signing_key = "defaultsigningkey"
refresh_signing_key = "defaultrefreshsigningkey"
# Basic-auth credential the SDK sends on Authenticate* and SessionRefresh.
server_key = "defaultkey"
token_expires_sec = 60
refresh_token_expires_sec = 3600

# Admin console (/dashboard): its own login, separate from player sessions.
# `console_user` rows in the database are checked first; username/password below
# is the bootstrap superadmin for a fresh database. Change both before shipping.
[control.console]
username = "admin"
password = "password"
signing_key = "defaultconsolesigningkey"
token_expires_sec = 3600
"#;

/// Historical flat-source template, kept for reference only. New projects use
/// [`ROOM_TEMPLATE`], which is a real `Room` class with the generated ABI
/// bridge, typed messages and an own-SDK website.
#[allow(dead_code)]
pub const LEGACY_ROOM_TEMPLATE: &str = r#"// Room movement logic — compiled to WebAssembly, run by the Rust host
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

/// Easy-mode AssemblyScript room used by `ox-game init`.
///
/// Three independent channels, deliberately:
/// - `this.on` / `this.emit` — local, in-process room events. Free and not
///   automatically networked.
/// - `onMessage` / `broadcast` / `sendTo` — typed, reliable network messages.
/// - `SchemaState` — continuous state, automatically full-synced then patched.
pub const ROOM_TEMPLATE: &str = include_str!("templates/easy_room.ts");

/// The state-sync schema `ox-game init` writes. Colyseus v5 `schema()`/`t.*`
/// syntax; `ox-game` compiles it into the typed AssemblyScript class plus the
/// browser descriptor, so the website needs no schema import at all.
pub const SCHEMA_TS_TEMPLATE: &str = include_str!("templates/easy_schema.ts");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easy_mode_defaults_to_30hz_and_20hz_remains_available() {
        let cfg = crate::parse_config(CONFIG_TEMPLATE);
        assert_eq!(
            cfg.tick_rate, 30,
            "the generated project is easy-mode 30 Hz"
        );
        assert!(
            cfg.permissions.messaging,
            "typed messages are on by default"
        );

        let twenty = crate::parse_config("tick_rate = 20\n");
        assert_eq!(twenty.tick_rate, 20, "20 Hz is still a supported choice");
    }

    #[test]
    fn room_template_is_a_class_with_all_three_channels() {
        assert!(ROOM_TEMPLATE.contains("export class GameRoom extends Room"));
        assert!(ROOM_TEMPLATE.contains("this.on(\"discard\""));
        assert!(ROOM_TEMPLATE.contains("this.onMessage(\"input\""));
        assert!(ROOM_TEMPLATE.contains("broadcastText("));
        assert!(ROOM_TEMPLATE.contains("room.sendTo("));
        assert!(ROOM_TEMPLATE.contains("from \"./cyg.schema\""));
        assert!(ROOM_TEMPLATE.contains("new SchemaState(PlayerSchema)"));
        assert!(!ROOM_TEMPLATE.contains("new SchemaDef("));
        assert!(ROOM_TEMPLATE.contains("return this.state.seal();"));
    }

    #[test]
    fn ts_schema_template_uses_colyseus_v5_syntax() {
        assert!(SCHEMA_TS_TEMPLATE.contains("from \"cyg\""));
        assert!(SCHEMA_TS_TEMPLATE.contains("const Player = schema({"));
        assert!(SCHEMA_TS_TEMPLATE.contains("players: t.map(Player)"));
    }
}
