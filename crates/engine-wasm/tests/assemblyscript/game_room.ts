// Real AssemblyScript game logic running with --runtime stub (Zero-GC)
let posX: f32 = 0.0;
let posY: f32 = 0.0;
let score: i32 = 0;

export function init(): void {
    posX = 100.0;
    posY = 100.0;
    score = 0;
}

export function update(dt: f32, speedX: f32, speedY: f32): void {
    posX += speedX * dt;
    posY += speedY * dt;
    score += 1;
}

export function getScore(): i32 {
    return score;
}

export function getPositionX(): f32 {
    return posX;
}

export function getPositionY(): f32 {
    return posY;
}

export function calculateDamage(base: i32, multiplier: f32): i32 {
    return i32(f32(base) * multiplier);
}
