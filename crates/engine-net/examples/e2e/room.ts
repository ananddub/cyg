// Room movement logic — compiled to WebAssembly (Generic Byte Pipeline ABI)
// Server-authoritative simulation with zero Host knowledge of game fields.

class Player {
    id: i64 = 0;
    x: f64 = 0;
    y: f64 = 0;
    flags: u32 = 0;
    score: f64 = 0;
}

var players: Player[] = new Array<Player>();
var max_players: i32 = 16;
var play_w: f64 = 800;
var play_h: f64 = 500;
var speed: f64 = 150;

// Golden coin
var coin_x: f64 = 400;
var coin_y: f64 = 250;
var rng_state: u64 = 0x9E3779B97F4A7C15;

function rand01(): f64 {
    rng_state ^= rng_state << 13;
    rng_state ^= rng_state >> 7;
    rng_state ^= rng_state << 17;
    return (rng_state >> 11) as f64 / 9007199254740992.0;
}

function respawn_coin(): void {
    coin_x = 40 + rand01() * (play_w - 80);
    coin_y = 40 + rand01() * (play_h - 80);
}

// IO Buffers in WASM linear memory
var input_buf = new ArrayBuffer(256);
var delta_buf = new ArrayBuffer(4096);

export function on_create(max: i32): void {
    max_players = max;
    players.length = 0;
    respawn_coin();
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

// Memory allocator for incoming host data
export function alloc(len: i32): i32 {
    if (len > input_buf.byteLength) {
        input_buf = new ArrayBuffer(len * 2);
    }
    return changetype<usize>(input_buf) as i32;
}

// Host passes raw input bytes
export function on_input(id: i64, ptr: i32, len: i32): void {
    let view = new DataView(input_buf);
    let flags: u32 = 0;
    if (len >= 4) {
        flags = view.getUint32(0, true);
    } else if (len >= 1) {
        flags = u32(view.getUint8(0));
    }

    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        if (p.id == id) {
            p.flags = flags;
            return;
        }
    }
}

// Fixed tick: updates game simulation and encodes binary delta
export function tick(dt: f64): i64 {
    // 1. Advance simulation
    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        let f = p.flags;
        if (f & 1) p.y -= speed * dt; // W
        if (f & 2) p.y += speed * dt; // S
        if (f & 4) p.x -= speed * dt; // A
        if (f & 8) p.x += speed * dt; // D

        if (p.x < 14) p.x = 14;
        if (p.x > play_w - 14) p.x = play_w - 14;
        if (p.y < 14) p.y = 14;
        if (p.y > play_h - 14) p.y = play_h - 14;

        // Coin pickup
        let dx = p.x - coin_x;
        let dy = p.y - coin_y;
        if (dx * dx + dy * dy < 24 * 24) {
            p.score += 10;
            respawn_coin();
        }
    }

    // 2. Binary delta serialization:
    // [u32 count]
    // for each player: [i64 id, f64 x, f64 y, f64 score]
    // [f64 coin_x, f64 coin_y]
    let needed = 4 + (players.length * 32) + 16;
    if (needed > delta_buf.byteLength) {
        delta_buf = new ArrayBuffer(needed * 2);
    }

    let view = new DataView(delta_buf);
    let offset = 0;

    view.setUint32(offset, u32(players.length), true);
    offset += 4;

    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        view.setInt64(offset, p.id, true);
        offset += 8;
        view.setFloat64(offset, p.x, true);
        offset += 8;
        view.setFloat64(offset, p.y, true);
        offset += 8;
        view.setFloat64(offset, p.score, true);
        offset += 8;
    }

    view.setFloat64(offset, coin_x, true);
    offset += 8;
    view.setFloat64(offset, coin_y, true);
    offset += 8;

    let ptr = changetype<usize>(delta_buf) as i64;
    let len = i64(offset);
    return (ptr << 32) | len;
}

export function save(): i64 {
    return 0;
}

export function load(ptr: i32, len: i32): void {
}
