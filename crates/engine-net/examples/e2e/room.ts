// Room movement logic — compiled to WebAssembly, run by the Rust host
// through the engine-wasm crate (server-authoritative simulation).
//
// Exports (host -> guest ABI, all numeric so the wrapper stays trivial):
//   on_join(id, x, y)              spawn a player
//   on_leave(id)                   remove a player
//   on_input(id, flags)            store held-key bitmask (W=1 S=2 A=4 D=8)
//   update(dt, w, h, speed)        advance the simulation one tick
//   get_count() / get_id(i) / get_x(i) / get_y(i)

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

/// Room rule lives here: is the room full?
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

        // clamp to playfield
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
