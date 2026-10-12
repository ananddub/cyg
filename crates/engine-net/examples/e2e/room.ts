// Room movement & shooting logic — compiled to WebAssembly (Generic Byte Pipeline ABI)
// Server-authoritative simulation with zero Host knowledge of game fields.

class Player {
    id: i64 = 0;
    x: f64 = 0;
    y: f64 = 0;
    flags: u32 = 0;
    aim_angle: f64 = 0;
    score: f64 = 0;
    hp: f64 = 100;
    cooldown: f64 = 0;
}

class Bullet {
    owner_id: i64 = 0;
    x: f64 = 0;
    y: f64 = 0;
    vx: f64 = 0;
    vy: f64 = 0;
    life: f64 = 1.2;
}

var players: Player[] = new Array<Player>();
var bullets: Bullet[] = new Array<Bullet>();
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
var delta_buf = new ArrayBuffer(8192);

export function on_create(max: i32): void {
    max_players = max;
    players.length = 0;
    bullets.length = 0;
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
    p.hp = 100.0;
    p.score = 0.0;
    p.cooldown = 0.0;
    p.aim_angle = 0.0;
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
    let angle: f64 = 0.0;

    if (len >= 8) {
        flags = view.getUint32(0, true);
        angle = f64(view.getFloat32(4, true));
    } else if (len >= 4) {
        flags = view.getUint32(0, true);
    } else if (len >= 1) {
        flags = u32(view.getUint8(0));
    }

    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        if (p.id == id) {
            p.flags = flags;
            p.aim_angle = angle;
            return;
        }
    }
}

// Fixed tick: updates game simulation and encodes binary delta
export function tick(dt: f64): i64 {
    // 1. Advance players
    for (let i = 0; i < players.length; i++) {
        let p = unchecked(players[i]);
        if (p.cooldown > 0.0) {
            p.cooldown -= dt;
        }

        let f = p.flags;
        if (f & 1) p.y -= speed * dt; // W
        if (f & 2) p.y += speed * dt; // S
        if (f & 4) p.x -= speed * dt; // A
        if (f & 8) p.x += speed * dt; // D

        if (p.x < 14.0) p.x = 14.0;
        if (p.x > play_w - 14.0) p.x = play_w - 14.0;
        if (p.y < 14.0) p.y = 14.0;
        if (p.y > play_h - 14.0) p.y = play_h - 14.0;

        // Shooting check: bit 4 (16) = SHOOT
        if ((f & 16) && p.cooldown <= 0.0) {
            let b = new Bullet();
            b.owner_id = p.id;
            b.x = p.x + Math.cos(p.aim_angle) * 16.0;
            b.y = p.y + Math.sin(p.aim_angle) * 16.0;
            b.vx = Math.cos(p.aim_angle) * 450.0;
            b.vy = Math.sin(p.aim_angle) * 450.0;
            b.life = 1.2;
            bullets.push(b);
            p.cooldown = 0.18; // ~5.5 bullets/sec
        }

        // Coin pickup
        let dx = p.x - coin_x;
        let dy = p.y - coin_y;
        if (dx * dx + dy * dy < 24.0 * 24.0) {
            p.score += 10.0;
            respawn_coin();
        }
    }

    // 2. Advance bullets & check collisions
    for (let i = bullets.length - 1; i >= 0; i--) {
        let b = unchecked(bullets[i]);
        b.x += b.vx * dt;
        b.y += b.vy * dt;
        b.life -= dt;

        if (b.life <= 0.0 || b.x < 0.0 || b.x > play_w || b.y < 0.0 || b.y > play_h) {
            bullets.splice(i, 1);
            continue;
        }

        // Collision against other players
        let hit = false;
        for (let j = 0; j < players.length; j++) {
            let p = unchecked(players[j]);
            if (p.id == b.owner_id) continue;

            let pdx = p.x - b.x;
            let pdy = p.y - b.y;
            if (pdx * pdx + pdy * pdy < 16.0 * 16.0) {
                // Hit!
                p.hp -= 20.0;
                hit = true;

                if (p.hp <= 0.0) {
                    // Killer gets score
                    for (let k = 0; k < players.length; k++) {
                        let shooter = unchecked(players[k]);
                        if (shooter.id == b.owner_id) {
                            shooter.score += 50.0;
                            break;
                        }
                    }
                    // Victim respawns
                    p.hp = 100.0;
                    p.x = 40.0 + rand01() * (play_w - 80.0);
                    p.y = 40.0 + rand01() * (play_h - 80.0);
                }
                break;
            }
        }

        if (hit) {
            bullets.splice(i, 1);
        }
    }

    // 3. Binary delta serialization:
    // [u32 player_count]
    // for each player: [i64 id, f64 x, f64 y, f64 score, f64 hp]  (40 bytes)
    // [f64 coin_x, f64 coin_y]                                     (16 bytes)
    // [u32 bullet_count]                                           (4 bytes)
    // for each bullet: [f64 x, f64 y, i64 owner_id]                (24 bytes)

    let needed = 4 + (players.length * 40) + 16 + 4 + (bullets.length * 24);
    if (needed > delta_buf.byteLength) {
        delta_buf = new ArrayBuffer(needed * 2);
    }

    let view = new DataView(delta_buf);
    let offset = 0;

    // Players
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
        view.setFloat64(offset, p.hp, true);
        offset += 8;
    }

    // Coin
    view.setFloat64(offset, coin_x, true);
    offset += 8;
    view.setFloat64(offset, coin_y, true);
    offset += 8;

    // Bullets
    view.setUint32(offset, u32(bullets.length), true);
    offset += 4;

    for (let i = 0; i < bullets.length; i++) {
        let b = unchecked(bullets[i]);
        view.setFloat64(offset, b.x, true);
        offset += 8;
        view.setFloat64(offset, b.y, true);
        offset += 8;
        view.setInt64(offset, b.owner_id, true);
        offset += 8;
    }

    let ptr = changetype<usize>(delta_buf) as i64;
    let len = i64(offset);
    return (ptr << 32) | len;
}

export function save(): i64 {
    return 0;
}

export function load(ptr: i32, len: i32): void {
}
