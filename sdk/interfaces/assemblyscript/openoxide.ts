// =============================================================================
// OpenOxide Game Server — High-Level Guest Room SDK for AssemblyScript
// WebAssembly Target: wasm32 (asc -Ospeed --runtime stub)
// Zero Low-Level Plumbing: No raw pointers, byte offsets, or endianness math!
// =============================================================================

// =============================================================================
// --- 1. INTERNAL HOST C-ABI DECLARATIONS (HIDDEN FROM DEVELOPER) ---
// =============================================================================

/**
 * Deterministic pseudo-random float in [0, 1) — engine-owned ChaCha8 stream,
 * seeded from the room name. Same room seed + same call order = identical
 * sequences on server and every replaying client. Use this INSTEAD of
 * Math.random anywhere gameplay randomness is involved (loot, spawns, crits).
 */
export function rngNext(): f64 {
    return _sys_rng_next();
}

// --- SPATIAL AOI (kiddo KD-tree, host-owned) -------------------------------
// Push each moving entity's position once per tick, then query neighbors.

@external("env", "sys_spatial_update")
declare function _sys_spatial_update(entity_id: i64, x: f64, y: f64): i32;

@external("env", "sys_spatial_query")
declare function _sys_spatial_query(x: f64, y: f64, radius: f64, out_ptr: usize, max_results: i32): i32;

/** Insert-or-move an entity in the room's AOI index (idempotent per tick). */
export function spatialUpdate(entityId: i64, x: f64, y: f64): void {
    _sys_spatial_update(entityId, x, y);
}

/**
 * Query entities within `radius` of (x, y). Fills `outIds` (u32 little-endian)
 * with up to outIds.length / 4 entity ids; returns the count written.
 */
export function spatialQuery(x: f64, y: f64, radius: f64, outIds: usize): i32 {
    return _sys_spatial_query(x, y, radius, outIds, outIds == 0 ? 0 : 9999);
}

// --- DETERMINISTIC CLOCK ---------------------------------------------------

@external("env", "sys_clock_ticks")
declare function _sys_clock_ticks(): i64;

/** The authoritative simulation tick counter (never wall-clock). */
export function clockTicks(): i64 {
    return _sys_clock_ticks();
}

// --- INPUT FLAGS (Phase-3-lite: no more raw bitmask decoding) ---------------

// AS has no object literals — static-class constants instead.
export class Key {
    static readonly W: u8 = 1;
    static readonly S: u8 = 2;
    static readonly A: u8 = 4;
    static readonly D: u8 = 8;
    static readonly Space: u8 = 16;
    static readonly Shift: u8 = 32;
}

/** First byte of an on_input payload as a held-key bitmask. */
export function heldFlags(ptr: usize, len: usize): u8 {
    return len > 0 ? load<u8>(ptr) : 0;
}

export function isHeld(flags: u8, key: u8): bool {
    return (flags & key) != 0;
}

// =============================================================================

// Deterministic room RNG — the HOST owns the ChaCha8 stream (seeded from the
// room name), so every client replays identical sequences with zero traffic.
// Never use Math.random in room logic: it breaks determinism.
@external("env", "sys_rng_next")
declare function _sys_rng_next(): f64;

@external("env", "sys_physics_move")
declare function _sys_physics_move(
    px: f64, py: f64, pz: f64,
    vx: f64, vy: f64, vz: f64,
    radius: f64, dt: f64,
    out_ptr: usize
): i32;

@external("env", "sys_physics_raycast")
declare function _sys_physics_raycast(
    ox: f64, oy: f64, oz: f64,
    dx: f64, dy: f64, dz: f64,
    max_dist: f64,
    out_ptr: usize
): i32;

@external("env", "sys_physics_surface_y")
declare function _sys_physics_surface_y(
    px: f64, py: f64, pz: f64,
    radius: f64
): f64;

@external("env", "sys_physics_lag_record")
declare function _sys_physics_lag_record(
    tick: i64,
    entity_id: i32,
    kind: i32,
    min_x: f64, min_y: f64, min_z: f64,
    max_x: f64, max_y: f64, max_z: f64
): void;

@external("env", "sys_physics_lag_raycast")
declare function _sys_physics_lag_raycast(
    shoot_tick: i64,
    ox: f64, oy: f64, oz: f64,
    dx: f64, dy: f64, dz: f64,
    max_dist: f64,
    out_ptr: usize
): i32;

@external("env", "host_log")
declare function _host_log(ptr: usize, len: i32): i32;

// Typed room message: target=0 broadcasts, target=player id sends to one player.
// The host frames it as [0x05][type_len u8][type][payload] and routes it.
@external("env", "host_send")
declare function _host_send(
    target: i64,
    type_ptr: usize,
    type_len: i32,
    data_ptr: usize,
    data_len: i32
): i32;

// --- RAPIER 2D PHYSICS (HOST-OWNED) ------------------------------------------

@external("env", "sys_rapier2d_init")
declare function _sys_rapier2d_init(gx: f64, gy: f64): i32;

@external("env", "sys_rapier2d_create_body")
declare function _sys_rapier2d_create_body(body_type: i32, x: f64, y: f64): i32;

@external("env", "sys_rapier2d_add_circle_collider")
declare function _sys_rapier2d_add_circle_collider(
    body_id: i32, radius: f64, is_sensor: i32, restitution: f64, friction: f64
): i32;

@external("env", "sys_rapier2d_add_box_collider")
declare function _sys_rapier2d_add_box_collider(
    body_id: i32, half_w: f64, half_h: f64, is_sensor: i32, restitution: f64, friction: f64
): i32;

@external("env", "sys_rapier2d_set_velocity")
declare function _sys_rapier2d_set_velocity(body_id: i32, vx: f64, vy: f64): i32;

@external("env", "sys_rapier2d_get_velocity")
declare function _sys_rapier2d_get_velocity(body_id: i32, out_ptr: usize): i32;

@external("env", "sys_rapier2d_get_position")
declare function _sys_rapier2d_get_position(body_id: i32, out_ptr: usize): i32;

@external("env", "sys_rapier2d_set_position")
declare function _sys_rapier2d_set_position(body_id: i32, x: f64, y: f64): i32;

@external("env", "sys_rapier2d_apply_impulse")
declare function _sys_rapier2d_apply_impulse(body_id: i32, ix: f64, iy: f64): i32;

@external("env", "sys_rapier2d_step")
declare function _sys_rapier2d_step(dt: f64): i32;

@external("env", "sys_rapier2d_check_contact")
declare function _sys_rapier2d_check_contact(body_id_a: i32, body_id_b: i32): i32;

@external("env", "sys_rapier2d_remove_body")
declare function _sys_rapier2d_remove_body(body_id: i32): i32;

// =============================================================================
// --- 2. VECTOR 2D & 3D MATH HELPERS ---
// =============================================================================

export class Vec2 {
    x: f64;
    y: f64;

    constructor(x: f64 = 0.0, y: f64 = 0.0) {
        this.x = x;
        this.y = y;
    }

    set(x: f64, y: f64): this {
        this.x = x;
        this.y = y;
        return this;
    }

    copy(other: Vec2): this {
        this.x = other.x;
        this.y = other.y;
        return this;
    }

    clone(): Vec2 {
        return new Vec2(this.x, this.y);
    }

    add(other: Vec2): Vec2 {
        return new Vec2(this.x + other.x, this.y + other.y);
    }

    sub(other: Vec2): Vec2 {
        return new Vec2(this.x - other.x, this.y - other.y);
    }

    scale(s: f64): Vec2 {
        return new Vec2(this.x * s, this.y * s);
    }

    length(): f64 {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }

    lengthSq(): f64 {
        return this.x * this.x + this.y * this.y;
    }

    distanceTo(other: Vec2): f64 {
        let dx = this.x - other.x;
        let dy = this.y - other.y;
        return Math.sqrt(dx * dx + dy * dy);
    }

    normalize(): Vec2 {
        let len = this.length();
        if (len > 0.00001) {
            return new Vec2(this.x / len, this.y / len);
        }
        return new Vec2(0.0, 0.0);
    }

    dot(other: Vec2): f64 {
        return this.x * other.x + this.y * other.y;
    }

    angleTo(other: Vec2): f64 {
        return Math.atan2(other.y - this.y, other.x - this.x);
    }

    static fromAngle(rad: f64, length: f64 = 1.0): Vec2 {
        return new Vec2(Math.cos(rad) * length, Math.sin(rad) * length);
    }

    toString(): string {
        return "(" + this.x.toString() + ", " + this.y.toString() + ")";
    }
}

export class Vec3 {
    x: f64;
    y: f64;
    z: f64;

    constructor(x: f64 = 0.0, y: f64 = 0.0, z: f64 = 0.0) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    set(x: f64, y: f64, z: f64): this {
        this.x = x;
        this.y = y;
        this.z = z;
        return this;
    }

    copy(other: Vec3): this {
        this.x = other.x;
        this.y = other.y;
        this.z = other.z;
        return this;
    }

    clone(): Vec3 {
        return new Vec3(this.x, this.y, this.z);
    }

    add(other: Vec3): Vec3 {
        return new Vec3(this.x + other.x, this.y + other.y, this.z + other.z);
    }

    sub(other: Vec3): Vec3 {
        return new Vec3(this.x - other.x, this.y - other.y, this.z - other.z);
    }

    scale(s: f64): Vec3 {
        return new Vec3(this.x * s, this.y * s, this.z * s);
    }

    length(): f64 {
        return Math.sqrt(this.x * this.x + this.y * this.y + this.z * this.z);
    }

    lengthSq(): f64 {
        return this.x * this.x + this.y * this.y + this.z * this.z;
    }

    distanceTo(other: Vec3): f64 {
        let dx = this.x - other.x;
        let dy = this.y - other.y;
        let dz = this.z - other.z;
        return Math.sqrt(dx * dx + dy * dy + dz * dz);
    }

    distance2D(other: Vec3): f64 {
        let dx = this.x - other.x;
        let dz = this.z - other.z;
        return Math.sqrt(dx * dx + dz * dz);
    }

    normalize(): Vec3 {
        let len = this.length();
        if (len > 0.00001) {
            return new Vec3(this.x / len, this.y / len, this.z / len);
        }
        return new Vec3(0.0, 0.0, 0.0);
    }

    dot(other: Vec3): f64 {
        return this.x * other.x + this.y * other.y + this.z * other.z;
    }

    cross(other: Vec3): Vec3 {
        return new Vec3(
            this.y * other.z - this.z * other.y,
            this.z * other.x - this.x * other.z,
            this.x * other.y - this.y * other.x
        );
    }

    /**
     * Calculates the horizontal facing yaw angle (in radians) towards target.
     */
    yawTo(target: Vec3): f64 {
        return Math.atan2(target.z - this.z, target.x - this.x);
    }

    /**
     * Creates a horizontal forward direction vector from yaw angle.
     */
    static fromYaw(yawRad: f64, speed: f64 = 1.0): Vec3 {
        return new Vec3(Math.cos(yawRad) * speed, 0.0, Math.sin(yawRad) * speed);
    }

    toVec2XZ(): Vec2 {
        return new Vec2(this.x, this.z);
    }

    toString(): string {
        return "(" + this.x.toString() + ", " + this.y.toString() + ", " + this.z.toString() + ")";
    }
}

// =============================================================================
// --- 2.5 BASE ENTITY / ACTOR CLASS ---
// =============================================================================

/**
 * Standard Authoritative Entity representation.
 * Pre-packages pos, vel, rot, health, and standard distance/facing methods.
 */
export class Entity {
    id: i64 = 0;
    pos: Vec3 = new Vec3();
    vel: Vec3 = new Vec3();
    rot: Vec3 = new Vec3();
    radius: f64 = 0.5;
    health: f64 = 100.0;
    shield: f64 = 0.0;
    armor: f64 = 0.0;
    isAlive: bool = true;

    distanceTo(other: Entity): f64 {
        return this.pos.distanceTo(other.pos);
    }

    lookAt(target: Vec3): void {
        this.rot.y = this.pos.yawTo(target);
    }
}

// =============================================================================
// --- 3. HIGH-LEVEL ZERO-GC PHYSICS & COLLISION SYSTEM ---
// =============================================================================

export class MoveResult {
    pos: Vec3 = new Vec3();
    x: f64 = 0;
    y: f64 = 0;
    z: f64 = 0;
    isGrounded: bool = false;
}

export class RaycastHit {
    hasHit: bool = false;
    point: Vec3 = new Vec3();
    x: f64 = 0;
    y: f64 = 0;
    z: f64 = 0;
    distance: f64 = 0;
    colliderId: u32 = 0;
}

export class LagRaycastHit {
    hasHit: bool = false;
    point: Vec3 = new Vec3();
    hitType: i32 = 0; // 0 = None, 1 = Static Wall, 2 = Entity Hitbox
    x: f64 = 0;
    y: f64 = 0;
    z: f64 = 0;
    distance: f64 = 0;
    entityId: u32 = 0;
    kind: u32 = 0;
    colliderId: u32 = 0;
}

export class CharacterStepResult {
    onGround: bool = true;
    isMoving: bool = false;
    bodyRot: f64 = 0.0;
}

export class SweptHitResult {
    hasHit: bool = false;
    hitEntityId: i64 = 0;
    t: f64 = 1.0;
    isHeadshot: bool = false;
    point: Vec3 = new Vec3();
}

/**
 * Universal High-Level Physics Engine for OpenOxide Game Rooms.
 * Handles continuous collision detection, circles, spheres, ramps, and obstacles seamlessly.
 * Supports both friendly Vector objects (Vec2 / Vec3) and raw coordinates.
 */
export class Physics {
    private static _buf: ArrayBuffer = new ArrayBuffer(64);
    private static _ptr: usize = changetype<usize>(Physics._buf);
    private static _view: DataView = new DataView(Physics._buf);

    private static _moveRes: MoveResult = new MoveResult();
    private static _rayHit: RaycastHit = new RaycastHit();
    private static _lagRayHit: LagRaycastHit = new LagRaycastHit();
    private static _charStepRes: CharacterStepResult = new CharacterStepResult();
    private static _sweptHitRes: SweptHitResult = new SweptHitResult();

    /**
     * Kinematic Move & Slide (Friendly Vec3):
     * Moves a cylinder/capsule entity (player/bot) along walls, ramps, and Blender obstacles.
     * @returns Pre-allocated MoveResult containing updated pos (Vec3) and isGrounded status.
     */
    static move(pos: Vec3, vel: Vec3, radius: f64, dt: f64): MoveResult {
        return Physics.moveRaw(pos.x, pos.y, pos.z, vel.x, vel.y, vel.z, radius, dt);
    }

    /**
     * Kinematic Move & Slide (Raw Coordinates):
     */
    static moveRaw(px: f64, py: f64, pz: f64, vx: f64, vy: f64, vz: f64, radius: f64, dt: f64): MoveResult {
        let grounded = _sys_physics_move(px, py, pz, vx, vy, vz, radius, dt, Physics._ptr);
        let res = Physics._moveRes;
        let nx = Physics._view.getFloat64(0, true);
        let ny = Physics._view.getFloat64(8, true);
        let nz = Physics._view.getFloat64(16, true);
        res.x = nx;
        res.y = ny;
        res.z = nz;
        res.pos.set(nx, ny, nz);
        res.isGrounded = (grounded == 1);
        return res;
    }

    /**
     * Continuous Collision Detection (CCD) 3D Raycast (Friendly Vec3):
     * Casts a ray through the 3D world against walls, bunkers, pillars, and Blender geometry.
     * @returns Pre-allocated RaycastHit with exact impact point (Vec3) and distance.
     */
    static raycast(origin: Vec3, direction: Vec3, maxDist: f64): RaycastHit {
        return Physics.raycastRaw(origin.x, origin.y, origin.z, direction.x, direction.y, direction.z, maxDist);
    }

    static raycastRaw(ox: f64, oy: f64, oz: f64, dx: f64, dy: f64, dz: f64, maxDist: f64): RaycastHit {
        let hit = _sys_physics_raycast(ox, oy, oz, dx, dy, dz, maxDist, Physics._ptr);
        let res = Physics._rayHit;
        if (hit != 0) {
            let hx = Physics._view.getFloat64(0, true);
            let hy = Physics._view.getFloat64(8, true);
            let hz = Physics._view.getFloat64(16, true);
            res.hasHit = true;
            res.x = hx;
            res.y = hy;
            res.z = hz;
            res.point.set(hx, hy, hz);
            res.distance = Physics._view.getFloat64(24, true);
            res.colliderId = Physics._view.getUint32(32, true);
        } else {
            res.hasHit = false;
            res.distance = maxDist;
            res.colliderId = 0;
            res.point.set(0, 0, 0);
        }
        return res;
    }

    /**
     * Floor, Ramp & Stair Surface Height (Friendly Vec3):
     * Returns the exact solid ground Y coordinate beneath an entity.
     */
    static getSurfaceHeight(pos: Vec3, radius: f64 = 0.0): f64 {
        return _sys_physics_surface_y(pos.x, pos.y, pos.z, radius);
    }

    static getSurfaceHeightRaw(px: f64, py: f64, pz: f64, radius: f64 = 0.0): f64 {
        return _sys_physics_surface_y(px, py, pz, radius);
    }

    // --- SHAPE & TRIGGER UTILITIES (CIRCLES, SPHERES, DISTANCES) ---

    /**
     * 3D Euclidean distance between two positions (Friendly Vec3).
     */
    static distance(a: Vec3, b: Vec3): f64 {
        return a.distanceTo(b);
    }

    static distanceRaw(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64): f64 {
        let dx = x1 - x2;
        let dy = y1 - y2;
        let dz = z1 - z2;
        return Math.sqrt(dx * dx + dy * dy + dz * dz);
    }

    /**
     * 2D Planar distance (Friendly Vec2).
     */
    static distance2D(a: Vec2, b: Vec2): f64 {
        return a.distanceTo(b);
    }

    static distance2DRaw(x1: f64, z1: f64, x2: f64, z2: f64): f64 {
        let dx = x1 - x2;
        let dz = z1 - z2;
        return Math.sqrt(dx * dx + dz * dz);
    }

    /**
     * 2D Circle Overlap: returns true if two circles intersect on the ground (Friendly Vec2).
     */
    static checkCircleOverlap(posA: Vec2, rA: f64, posB: Vec2, rB: f64): bool {
        let dx = posA.x - posB.x;
        let dy = posA.y - posB.y;
        let minDist = rA + rB;
        return (dx * dx + dy * dy) <= (minDist * minDist);
    }

    static checkCircleOverlapRaw(x1: f64, z1: f64, r1: f64, x2: f64, z2: f64, r2: f64): bool {
        let dx = x1 - x2;
        let dz = z1 - z2;
        let minDist = r1 + r2;
        return (dx * dx + dz * dz) <= (minDist * minDist);
    }

    /**
     * 3D Sphere Overlap: returns true if two spheres intersect in 3D space (Friendly Vec3).
     */
    static checkSphereOverlap(posA: Vec3, rA: f64, posB: Vec3, rB: f64): bool {
        return posA.distanceTo(posB) <= (rA + rB);
    }

    /**
     * Point in Sphere: checks if a point lies within a spherical blast radius (Friendly Vec3).
     */
    static isPointInSphere(point: Vec3, center: Vec3, radius: f64): bool {
        return point.distanceTo(center) <= radius;
    }

    static isPointInSphereRaw(px: f64, py: f64, pz: f64, cx: f64, cy: f64, cz: f64, radius: f64): bool {
        let dx = px - cx;
        let dy = py - cy;
        let dz = pz - cz;
        return (dx * dx + dy * dy + dz * dz) <= (radius * radius);
    }

    /**
     * Point in AABB Box: checks if a point lies inside an axis-aligned bounding box (Friendly Vec3).
     */
    static isPointInBox(point: Vec3, min: Vec3, max: Vec3): bool {
        return point.x >= min.x && point.x <= max.x &&
               point.y >= min.y && point.y <= max.y &&
               point.z >= min.z && point.z <= max.z;
    }

    static isPointInBoxRaw(
        px: f64, py: f64, pz: f64,
        minX: f64, minY: f64, minZ: f64,
        maxX: f64, maxY: f64, maxZ: f64
    ): bool {
        return px >= minX && px <= maxX &&
               py >= minY && py <= maxY &&
               pz >= minZ && pz <= maxZ;
    }

    /**
     * Oriented Bounding Box (OBB): checks if a point lies inside a rotated 3D/2D box (Friendly Vec3).
     * Essential for Blender buildings, shipping containers, and vehicles rotated at arbitrary angles!
     * @param yawRad Yaw rotation of the box in radians.
     */
    static isPointInRotatedBox(pos: Vec3, center: Vec3, halfWidth: f64, halfDepth: f64, yawRad: f64): bool {
        return Physics.isPointInRotatedBoxRaw(pos.x, pos.z, center.x, center.z, halfWidth, halfDepth, yawRad);
    }

    static isPointInRotatedBoxRaw(
        px: f64, pz: f64,
        cx: f64, cz: f64,
        halfWidth: f64, halfDepth: f64,
        yawRad: f64
    ): bool {
        let dx = px - cx;
        let dz = pz - cz;
        let cosA = Math.cos(-yawRad);
        let sinA = Math.sin(-yawRad);
        let localX = dx * cosA - dz * sinA;
        let localZ = dx * sinA + dz * cosA;
        return Math.abs(localX) <= halfWidth && Math.abs(localZ) <= halfDepth;
    }

    /**
     * Point in Vertical Capsule: standard character hitbox shape (Friendly Vec3).
     * Prevents players from snagging on stairs and uneven terrain edges.
     */
    static isPointInCapsule(point: Vec3, capPos: Vec3, radius: f64, height: f64): bool {
        return Physics.isPointInCapsuleRaw(point.x, point.y, point.z, capPos.x, capPos.y, capPos.z, radius, height);
    }

    static isPointInCapsuleRaw(
        px: f64, py: f64, pz: f64,
        capX: f64, capY: f64, capZ: f64,
        radius: f64, height: f64
    ): bool {
        let clampedY = Math.max(capY, Math.min(capY + height, py));
        let dx = px - capX;
        let dy = py - clampedY;
        let dz = pz - capZ;
        return (dx * dx + dy * dy + dz * dz) <= (radius * radius);
    }

    /**
     * Records an entity's 3D hitbox into host history for rewind lag compensation (Friendly Vec3).
     * Call this in onTick for every active player or bot.
     */
    static recordHitbox(tick: i64, entityId: i32, kind: i32, min: Vec3, max: Vec3): void {
        _sys_physics_lag_record(tick, entityId, kind, min.x, min.y, min.z, max.x, max.y, max.z);
    }

    static recordHitboxRaw(
        tick: i64,
        entityId: i32,
        kind: i32,
        minX: f64, minY: f64, minZ: f64,
        maxX: f64, maxY: f64, maxZ: f64
    ): void {
        _sys_physics_lag_record(tick, entityId, kind, minX, minY, minZ, maxX, maxY, maxZ);
    }

    /**
     * Rewind Lag-Compensated Raycast (Friendly Vec3):
     * Rewinds entity hitboxes to client's shoot_tick and performs a continuous raycast.
     * Guarantees "what you see is what you hit" without client-side trust exploits!
     */
    static lagRaycast(shootTick: i64, origin: Vec3, direction: Vec3, maxDist: f64): LagRaycastHit {
        return Physics.lagRaycastRaw(shootTick, origin.x, origin.y, origin.z, direction.x, direction.y, direction.z, maxDist);
    }

    static lagRaycastRaw(
        shootTick: i64,
        ox: f64, oy: f64, oz: f64,
        dx: f64, dy: f64, dz: f64,
        maxDist: f64
    ): LagRaycastHit {
        let hitType = _sys_physics_lag_raycast(shootTick, ox, oy, oz, dx, dy, dz, maxDist, Physics._ptr);
        let res = Physics._lagRayHit;
        if (hitType != 0) {
            let hx = Physics._view.getFloat64(0, true);
            let hy = Physics._view.getFloat64(8, true);
            let hz = Physics._view.getFloat64(16, true);
            res.hasHit = true;
            res.hitType = hitType;
            res.x = hx;
            res.y = hy;
            res.z = hz;
            res.point.set(hx, hy, hz);
            res.distance = Physics._view.getFloat64(24, true);
            res.entityId = Physics._view.getUint32(32, true);
            res.kind = Physics._view.getUint32(36, true);
            res.colliderId = Physics._view.getUint32(40, true);
        } else {
            res.hasHit = false;
            res.hitType = 0;
            res.distance = maxDist;
            res.entityId = 0;
            res.kind = 0;
            res.colliderId = 0;
            res.point.set(0, 0, 0);
        }
        return res;
    }

    /**
     * Complete Authoritative 3D Kinematics Step for Humanoid/Player Entities.
     * Automatically handles:
     * 1. Ground detection & solid surface snapping (getSurfaceHeight)
     * 2. Jump impulses & vertical gravity integration
     * 3. Normalized planar horizontal movement & speed scaling
     * 4. Kinematic move_and_slide along solid obstacle walls
     * 5. Smooth body facing rotation tracking
     * 6. Re-snapping to ground after horizontal sliding
     */
    static stepHumanoid(
        pos: Vec3,
        vel: Vec3,
        moveDir: Vec2,
        aimYaw: f64,
        bodyRot: f64,
        radius: f64,
        speed: f64,
        jumpForce: f64,
        gravity: f64,
        wantJump: bool,
        dt: f64
    ): CharacterStepResult {
        let res = Physics._charStepRes;

        // 1. Surface height & initial ground check
        let surfaceY = Physics.getSurfaceHeight(pos, radius);
        let onGround = (pos.y <= surfaceY + 0.1 && vel.y <= 0.0);
        if (onGround) {
            pos.y = surfaceY;
            vel.y = 0.0;
        }

        // 2. Jump impulse
        if (wantJump && onGround) {
            vel.y = jumpForce;
            onGround = false;
        }

        // 3. Gravity integration
        if (!onGround || vel.y > 0.0) {
            pos.y += vel.y * dt - 0.5 * gravity * dt * dt;
            vel.y -= gravity * dt;
            let currentSurface = Physics.getSurfaceHeight(pos, radius);
            if (pos.y <= currentSurface) {
                pos.y = currentSurface;
                vel.y = 0.0;
                onGround = true;
            }
        }

        // 4. Planar movement & smooth body facing rotation
        let mx = moveDir.x;
        let mz = moveDir.y;
        let isMoving = false;
        let vx: f64 = 0.0;
        let vz: f64 = 0.0;
        let newBodyRot = bodyRot;

        if (mx != 0.0 || mz != 0.0) {
            let len = Math.sqrt(mx * mx + mz * mz);
            if (len > 0.01) {
                isMoving = true;
                let nx = mx / len;
                let nz = mz / len;
                vx = nx * speed;
                vz = nz * speed;

                let targetRot = Math.atan2(-nx, -nz);
                let diff = targetRot - newBodyRot;
                while (diff < -3.14159265) diff += 6.2831853;
                while (diff > 3.14159265) diff -= 6.2831853;
                newBodyRot += diff * Math.min(1.0, 16.0 * dt);
            }
        } else {
            let diff = aimYaw - newBodyRot;
            while (diff < -3.14159265) diff += 6.2831853;
            while (diff > 3.14159265) diff -= 6.2831853;
            newBodyRot += diff * Math.min(1.0, 10.0 * dt);
        }

        // 5. Kinematic move along obstacles & walls
        vel.x = vx;
        vel.z = vz;
        let moveRes = Physics.move(pos, vel, radius, dt);
        pos.x = moveRes.pos.x;
        pos.z = moveRes.pos.z;

        // 6. Surface re-check after sliding
        let afterSurface = Physics.getSurfaceHeight(pos, radius);
        if (pos.y > afterSurface + 0.1) {
            onGround = false;
        } else if (pos.y < afterSurface) {
            pos.y = afterSurface;
            vel.y = 0.0;
            onGround = true;
        }

        res.onGround = onGround;
        res.isMoving = isMoving;
        res.bodyRot = newBodyRot;
        return res;
    }

    /**
     * Continuous Collision Detection (CCD) Swept Segment against Entity Cylinders.
     * Checks if a moving projectile trajectory (from -> to) intersects any entity's vertical cylinder.
     * Includes headshot detection and wall occluder distance checks!
     */
    static sweepSegmentAgainstEntities<T extends Entity>(
        from: Vec3,
        to: Vec3,
        entities: Array<T>,
        ignoreId: i64,
        hitRadius: f64,
        maxWallDist: f64 = 999999.0
    ): SweptHitResult {
        let res = Physics._sweptHitRes;
        res.hasHit = false;
        res.hitEntityId = 0;
        res.t = 2.0;
        res.isHeadshot = false;

        let segX = to.x - from.x;
        let segY = to.y - from.y;
        let segZ = to.z - from.z;
        let segLenSq = segX * segX + segZ * segZ;
        let segLen = Math.sqrt(segLenSq);
        let radiusSq = hitRadius * hitRadius;

        for (let i = 0; i < entities.length; i++) {
            let ent = entities[i];
            if (ent.id == ignoreId) continue;

            let entHeight: f64 = 19.5;
            let headThreshold: f64 = ent.pos.y + 14.5;

            let t: f64 = 0.0;
            if (segLenSq > 0.0001) {
                let toPx = ent.pos.x - from.x;
                let toPz = ent.pos.z - from.z;
                t = (toPx * segX + toPz * segZ) / segLenSq;
                if (t < 0.0) t = 0.0;
                if (t > 1.0) t = 1.0;
            }

            let closeX = from.x + segX * t;
            let closeY = from.y + segY * t;
            let closeZ = from.z + segZ * t;

            let dx = ent.pos.x - closeX;
            let dz = ent.pos.z - closeZ;
            let distSq = dx * dx + dz * dz;

            if (distSq <= radiusSq) {
                if (closeY >= ent.pos.y - 0.5 && closeY <= ent.pos.y + entHeight + 0.5) {
                    let travelDist = t * segLen;
                    if (travelDist <= maxWallDist + 0.5) {
                        if (t < res.t) {
                            res.hasHit = true;
                            res.hitEntityId = ent.id;
                            res.t = t;
                            res.point.set(closeX, closeY, closeZ);
                            res.isHeadshot = (closeY >= headThreshold);
                        }
                    }
                }
            }
        }

        return res;
    }

    /**
     * Resolves 2D planar cylinder crowd separation between entities so they don't clip into each other.
     * Zero-GC: modifies entity positions directly in-place.
     */
    static separateEntities<T extends Entity>(entities: Array<T>, radius: f64, maxVerticalDiff: f64 = 14.0): void {
        let count = entities.length;
        let minDist = radius * 2.0;
        let minDistSq = minDist * minDist;

        for (let i = 0; i < count; i++) {
            let a = entities[i];
            for (let j = i + 1; j < count; j++) {
                let b = entities[j];
                if (Math.abs(a.pos.y - b.pos.y) < maxVerticalDiff) {
                    let dx = b.pos.x - a.pos.x;
                    let dz = b.pos.z - a.pos.z;
                    let distSq = dx * dx + dz * dz;
                    if (distSq < minDistSq && distSq > 0.0001) {
                        let dist = Math.sqrt(distSq);
                        let overlap = (minDist - dist) * 0.5;
                        let nx = dx / dist;
                        let nz = dz / dist;
                        a.pos.x -= nx * overlap;
                        a.pos.z -= nz * overlap;
                        b.pos.x += nx * overlap;
                        b.pos.z += nz * overlap;
                    }
                }
            }
        }
    }
}

// =============================================================================
// --- 3.5 AUTHORITATIVE HOST RAPIER 2D RIGID-BODY PHYSICS ---
// =============================================================================

export enum BodyType {
    Dynamic = 0,
    Fixed = 1,
    KinematicVelocityBased = 2,
    KinematicPositionBased = 3
}

export class Rapier2D {
    private static _buf: ArrayBuffer = new ArrayBuffer(32);
    private static _ptr: usize = changetype<usize>(Rapier2D._buf);
    private static _view: DataView = new DataView(Rapier2D._buf);

    /** Initialize host Rapier2D physics world with gravity (gx, gy). */
    static init(gx: f64 = 0.0, gy: f64 = 0.0): void {
        _sys_rapier2d_init(gx, gy);
    }

    /** Create a rigid body (Dynamic, Fixed, Kinematic) at (x, y). Returns body ID. */
    static createBody(type: BodyType, x: f64, y: f64): i32 {
        return _sys_rapier2d_create_body(type as i32, x, y);
    }

    /** Attach a circle/ball collider to a body. */
    static addCircleCollider(
        bodyId: i32,
        radius: f64,
        isSensor: bool = false,
        restitution: f64 = 0.0,
        friction: f64 = 0.5
    ): i32 {
        return _sys_rapier2d_add_circle_collider(
            bodyId,
            radius,
            isSensor ? 1 : 0,
            restitution,
            friction
        );
    }

    /** Attach a box/cuboid collider to a body. */
    static addBoxCollider(
        bodyId: i32,
        halfW: f64,
        halfH: f64,
        isSensor: bool = false,
        restitution: f64 = 0.0,
        friction: f64 = 0.5
    ): i32 {
        return _sys_rapier2d_add_box_collider(
            bodyId,
            halfW,
            halfH,
            isSensor ? 1 : 0,
            restitution,
            friction
        );
    }

    /** Set linear velocity (vx, vy) on a dynamic or velocity-based kinematic body. */
    static setVelocity(bodyId: i32, vx: f64, vy: f64): void {
        _sys_rapier2d_set_velocity(bodyId, vx, vy);
    }

    /** Read current linear velocity (vx, vy) of a body into out Vec2. */
    static getVelocity(bodyId: i32, out: Vec2): bool {
        let res = _sys_rapier2d_get_velocity(bodyId, Rapier2D._ptr);
        if (res == 0) {
            out.x = Rapier2D._view.getFloat64(0, true);
            out.y = Rapier2D._view.getFloat64(8, true);
            return true;
        }
        return false;
    }

    /** Read authoritative (x, y) position of a body into out Vec2. */
    static getPosition(bodyId: i32, out: Vec2): bool {
        let res = _sys_rapier2d_get_position(bodyId, Rapier2D._ptr);
        if (res == 0) {
            out.x = Rapier2D._view.getFloat64(0, true);
            out.y = Rapier2D._view.getFloat64(8, true);
            return true;
        }
        return false;
    }

    /** Teleport or set position (x, y) of a body. */
    static setPosition(bodyId: i32, x: f64, y: f64): void {
        _sys_rapier2d_set_position(bodyId, x, y);
    }

    /** Apply an instantaneous linear impulse (ix, iy) to a dynamic body. */
    static applyImpulse(bodyId: i32, ix: f64, iy: f64): void {
        _sys_rapier2d_apply_impulse(bodyId, ix, iy);
    }

    /** Step the authoritative Rapier2D physics simulation by dt seconds. */
    static step(dt: f64): void {
        _sys_rapier2d_step(dt);
    }

    /** Return true if any collider of bodyA touches or intersects any collider of bodyB. */
    static checkContact(bodyA: i32, bodyB: i32): bool {
        return _sys_rapier2d_check_contact(bodyA, bodyB) == 1;
    }

    /** Remove a body and all its attached colliders from the physics world. */
    static removeBody(bodyId: i32): void {
        _sys_rapier2d_remove_body(bodyId);
    }
}

// =============================================================================
// --- 4. ZERO-SETUP HOST LOGGING (NO @external NEEDED) ---
// =============================================================================

export class Log {
    static info(message: string): void {
        let utf8 = String.UTF8.encode(message);
        _host_log(changetype<usize>(utf8), utf8.byteLength);
    }

    static warn(message: string): void {
        let utf8 = String.UTF8.encode("[WARN] " + message);
        _host_log(changetype<usize>(utf8), utf8.byteLength);
    }

    static error(message: string): void {
        let utf8 = String.UTF8.encode("[ERROR] " + message);
        _host_log(changetype<usize>(utf8), utf8.byteLength);
    }
}

// =============================================================================
// --- 5. TYPE-SAFE DELTA SERIALIZER (NO BITSHIFTS OR OFFSET MATH) ---
// =============================================================================

/**
 * Reusable binary delta writer.
 * Handles buffer growth, byte writes, and automatically packs (ptr, len) for onTick!
 */
export class DeltaWriter {
    private _buf: ArrayBuffer;
    private _view: DataView = new DataView(new ArrayBuffer(0));
    private _offset: i32 = 0;

    constructor(initialCapacity: i32 = 1024) {
        let buf = new ArrayBuffer(initialCapacity);
        this._buf = buf;
        this._view = new DataView(buf);
    }

    /** [tag][len][id u64] header. fieldBytes = id + saare field bytes
     *  (id ke baad jo likhoge) — developer byte math NAHI karta. */
    beginEntity(tag: u8, fieldBytes: u8, id: u64): this {
        this.writeU8(tag);
        this.writeU8(fieldBytes);
        this.writeI64(id as i64);
        return this;
    }

    reset(): this {
        this._offset = 0;
        return this;
    }

    ensureCapacity(neededBytes: i32): void {
        if (this._offset + neededBytes > this._buf.byteLength) {
            let doubleCap: i32 = this._buf.byteLength * 2;
            let targetCap: i32 = this._offset + neededBytes;
            let newCap: i32 = doubleCap > targetCap ? doubleCap : targetCap;
            let newBuf = new ArrayBuffer(newCap);
            let newView = new DataView(newBuf);
            // Copy existing
            for (let i = 0; i < this._offset; i++) {
                newView.setUint8(i, this._view.getUint8(i));
            }
            this._buf = newBuf;
            this._view = newView;
        }
    }

    writeU8(val: u8): this {
        this.ensureCapacity(1);
        this._view.setUint8(this._offset, val);
        this._offset += 1;
        return this;
    }

    writeI8(val: i8): this {
        this.ensureCapacity(1);
        this._view.setInt8(this._offset, val);
        this._offset += 1;
        return this;
    }

    writeI16(val: i16): this {
        this.ensureCapacity(2);
        this._view.setInt16(this._offset, val, true);
        this._offset += 2;
        return this;
    }

    writeU16(val: u16): this {
        this.ensureCapacity(2);
        this._view.setUint16(this._offset, val, true);
        this._offset += 2;
        return this;
    }

    writeU32(val: u32): this {
        this.ensureCapacity(4);
        this._view.setUint32(this._offset, val, true);
        this._offset += 4;
        return this;
    }

    writeI64(val: i64): this {
        this.ensureCapacity(8);
        this._view.setInt64(this._offset, val, true);
        this._offset += 8;
        return this;
    }

    writeF32(val: f32): this {
        this.ensureCapacity(4);
        this._view.setFloat32(this._offset, val, true);
        this._offset += 4;
        return this;
    }

    writeF64(val: f64): this {
        this.ensureCapacity(8);
        this._view.setFloat64(this._offset, val, true);
        this._offset += 8;
        return this;
    }

    writeI32(val: i32): this {
        this.ensureCapacity(4);
        this._view.setInt32(this._offset, val, true);
        this._offset += 4;
        return this;
    }

    writeBool(val: bool): this {
        this.ensureCapacity(1);
        this._view.setUint8(this._offset, val ? 1 : 0);
        this._offset += 1;
        return this;
    }

    /** Append an existing byte buffer (used by the schema encoder). */
    writeBytes(src: ArrayBuffer): this {
        let bytes = Uint8Array.wrap(src);
        this.ensureCapacity(bytes.length);
        let dst = Uint8Array.wrap(this._buf);
        dst.set(bytes, this._offset);
        this._offset += bytes.length;
        return this;
    }

    /** Append another writer's written region without allocating a copy. */
    writeWriter(src: DeltaWriter): this {
        this.ensureCapacity(src.length);
        let dst = Uint8Array.wrap(this._buf);
        let bytes = Uint8Array.wrap(src.buffer, 0, src.length);
        dst.set(bytes, this._offset);
        this._offset += src.length;
        return this;
    }

    /** Number of bytes written since the last reset. */
    get length(): i32 {
        return this._offset;
    }

    /** Backing buffer (may be larger than `length`). */
    get buffer(): ArrayBuffer {
        return this._buf;
    }

    /** Writes a 2D vector as two 32-bit floats (8 bytes total) */
    writeVec2(v: Vec2): this {
        this.writeF32(v.x as f32);
        this.writeF32(v.y as f32);
        return this;
    }

    /** Writes a 3D vector as three 32-bit floats (12 bytes total) */
    writeVec3(v: Vec3): this {
        this.writeF32(v.x as f32);
        this.writeF32(v.y as f32);
        this.writeF32(v.z as f32);
        return this;
    }

    /**
     * Finishes serialization and returns the packed 64-bit ABI integer for onTick / onSave!
     */
    seal(): i64 {
        let ptr = changetype<usize>(this._buf) as i64;
        let len = this._offset as i64;
        return (ptr << 32) | len;
    }
}

// =============================================================================
// --- 5.1 SCHEMA STATE SYNC (COLYSEUS-STYLE, OWN WIRE FORMAT) ---
// =============================================================================
//
// Define the schema once on the server and once on the client. The server
// stores values in a compact binary row per entity; setters mark fields dirty
// and `SchemaState.seal()` emits a full snapshot plus a field-level patch.
// The host sends the full snapshot to a client once, then patches.
//
// Wire (inside the delta body, after the recipient id):
//   full:  "OXS2" [u8 1] [u32 schema hash] [u32 count] { [u32 id] [values...] }*
//   patch: "OXS2" [u8 2] [u32 schema hash] [u32 ops]   { op... }*
//   conjoined onTick payload:
//          "OXS2" [u32 fullLen] [full] [u32 patchLen] [patch]
//
// Values: numeric/bool primitives, fixed-width UTF-8 strings (255 bytes) and
// flat primitive arrays, <= 32 fields per entity. Nested schemas stay on the
// typed-message channel.
//
// Field flags: bit 0 = optional (one presence byte on the wire), bit 1 = static
// and bit 2 = transient (both full-snapshot only, never patched).

export enum FieldType {
    U8 = 1,
    U16 = 2,
    U32 = 3,
    I32 = 4,
    F32 = 5,
    F64 = 6,
    BOOL = 7,
    I64 = 8,
    I8 = 9,
    I16 = 10,
    STRING = 11,
    ARRAY = 12,
}

export const SCHEMA_FIELD_OPTIONAL: u8 = 1;
export const SCHEMA_FIELD_STATIC: u8 = 2;
export const SCHEMA_FIELD_TRANSIENT: u8 = 4;

const SCHEMA_MAGIC_0: u8 = 0x4F; // O
const SCHEMA_MAGIC_1: u8 = 0x58; // X
const SCHEMA_MAGIC_2: u8 = 0x53; // S
const SCHEMA_MAGIC_3: u8 = 0x32; // 2
const SCHEMA_MAX_FIELDS: i32 = 32;

function schemaFieldSize(type: u8): i32 {
    if (type == FieldType.U8 || type == FieldType.I8 || type == FieldType.BOOL) return 1;
    if (type == FieldType.U16 || type == FieldType.I16) return 2;
    if (type == FieldType.U32 || type == FieldType.I32 || type == FieldType.F32) return 4;
    if (type == FieldType.STRING || type == FieldType.ARRAY) return 0;
    return 8; // F64 / I64
}

function schemaArraysEqual(a: Array<f64>, b: Array<f64>): bool {
    if (a.length != b.length) return false;
    for (let i = 0; i < a.length; i++) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

function writeSchemaMagic(w: DeltaWriter): void {
    w.writeU8(SCHEMA_MAGIC_0);
    w.writeU8(SCHEMA_MAGIC_1);
    w.writeU8(SCHEMA_MAGIC_2);
    w.writeU8(SCHEMA_MAGIC_3);
}

/**
 * Declarative field list shared by the server and the browser client.
 *
 * ```ts
 * const PlayerSchema = new SchemaDef("Player", 1)
 *   .u32("id")
 *   .f64("x")
 *   .f64("y")
 *   .u32("flags");
 * ```
 */
export class SchemaDef {
    private _name: string;
    private _version: u32;
    private _names: Array<string> = new Array<string>();
    private _types: Array<u8> = new Array<u8>();
    private _flags: Array<u8> = new Array<u8>();
    private _elements: Array<u8> = new Array<u8>();
    private _offsets: Array<i32> = new Array<i32>();
    private _sizes: Array<i32> = new Array<i32>();
    private _byteSize: i32 = 0;
    private _hash: u32;

    constructor(name: string, version: u32 = 1) {
        this._name = name;
        this._version = version;
        this._rehash();
    }

    private mix(hash: u32, text: string): u32 {
        let bytes = String.UTF8.encode(text);
        let ptr = changetype<usize>(bytes);
        let h = hash;
        for (let i = 0; i < bytes.byteLength; i++) {
            h = h ^ (load<u8>(ptr + i) as u32);
            h = h * 16777619;
        }
        return h;
    }

    private _rehash(): void {
        let h: u32 = 2166136261;
        h = this.mix(h, this._name);
        h = this.mix(h, "|");
        h = this.mix(h, this._version.toString());
        for (let i = 0; i < this._names.length; i++) {
            h = this.mix(h, "|");
            h = this.mix(h, this._names[i]);
            h = this.mix(h, ":");
            h = this.mix(h, this._types[i].toString());
            let element = this._elements[i];
            if (element != 0) {
                h = this.mix(h, ".");
                h = this.mix(h, element.toString());
            }
            let flags = this._flags[i];
            if (flags != 0) {
                h = this.mix(h, ":");
                h = this.mix(h, flags.toString());
            }
        }
        this._hash = h;
    }

    private add(name: string, type: u8, element: u8 = 0): this {
        if (this._names.length >= SCHEMA_MAX_FIELDS) return this;
        let size = schemaFieldSize(type);
        this._names.push(name);
        this._types.push(type);
        this._flags.push(0);
        this._elements.push(element);
        this._offsets.push(this._byteSize);
        this._sizes.push(size);
        this._byteSize += size;
        this._rehash();
        return this;
    }

    private _lastIndex(): i32 { return this._names.length - 1; }

    /** Marks the most recently added field as optional (presence byte). */
    optional(): this {
        let i = this._lastIndex();
        if (i >= 0) {
            this._flags[i] = (this._flags[i] | SCHEMA_FIELD_OPTIONAL) as u8;
            this._rehash();
        }
        return this;
    }

    /** Marks the most recently added field as full-snapshot only. */
    staticField(): this {
        let i = this._lastIndex();
        if (i >= 0) {
            this._flags[i] = (this._flags[i] | SCHEMA_FIELD_STATIC) as u8;
            this._rehash();
        }
        return this;
    }

    /** Marks the most recently added field as transient (full-snapshot only). */
    transient(): this {
        let i = this._lastIndex();
        if (i >= 0) {
            this._flags[i] = (this._flags[i] | SCHEMA_FIELD_TRANSIENT) as u8;
            this._rehash();
        }
        return this;
    }

    u8(name: string): this { return this.add(name, FieldType.U8 as u8); }
    i8(name: string): this { return this.add(name, FieldType.I8 as u8); }
    u16(name: string): this { return this.add(name, FieldType.U16 as u8); }
    i16(name: string): this { return this.add(name, FieldType.I16 as u8); }
    u32(name: string): this { return this.add(name, FieldType.U32 as u8); }
    i32(name: string): this { return this.add(name, FieldType.I32 as u8); }
    f32(name: string): this { return this.add(name, FieldType.F32 as u8); }
    f64(name: string): this { return this.add(name, FieldType.F64 as u8); }
    bool(name: string): this { return this.add(name, FieldType.BOOL as u8); }
    i64(name: string): this { return this.add(name, FieldType.I64 as u8); }
    string(name: string): this { return this.add(name, FieldType.STRING as u8); }
    array(name: string, elementType: u8): this { return this.add(name, FieldType.ARRAY as u8, elementType); }

    get hash(): u32 { return this._hash; }
    get version(): u32 { return this._version; }
    get name(): string { return this._name; }
    get fieldCount(): i32 { return this._names.length; }
    get byteSize(): i32 { return this._byteSize; }

    fieldName(index: i32): string { return this._names[index]; }
    fieldType(index: i32): u8 { return this._types[index]; }
    fieldFlags(index: i32): u8 { return this._flags[index]; }
    fieldElement(index: i32): u8 { return this._elements[index]; }
    fieldOffset(index: i32): i32 { return this._offsets[index]; }
    fieldSize(index: i32): i32 { return this._sizes[index]; }
    isOptional(index: i32): bool { return (this._flags[index] & SCHEMA_FIELD_OPTIONAL) != 0; }
    isFullOnly(index: i32): bool {
        return (this._flags[index] & (SCHEMA_FIELD_STATIC | SCHEMA_FIELD_TRANSIENT)) != 0;
    }

    /** Stable index for a field name, resolved once at module load. */
    indexOf(name: string): i32 {
        for (let i = 0; i < this._names.length; i++) {
            if (this._names[i] == name) return i;
        }
        return -1;
    }
}

/**
 * One row in a schema state. Subclass it and expose typed getters/setters:
 *
 * ```ts
 * const X = PlayerSchema.indexOf("x");
 *
 * class Player extends SchemaEntity {
 *   constructor() { super(PlayerSchema); }
 *   get x(): f64 { return this.getF64(X); }
 *   set x(v: f64) { this.setF64(X, v); }
 * }
 * ```
 *
 * Setters only mark the field dirty when the value actually changes, so a
 * no-op tick produces an empty patch.
 */
export class SchemaEntity {
    private _schema: SchemaDef;
    private _buf: ArrayBuffer;
    private _view: DataView = new DataView(new ArrayBuffer(0));
    private _dirty: u32 = 0;
    private _present: u32 = 0;
    private _strings: Array<string> = new Array<string>();
    private _arrays: Array<Array<f64>> = new Array<Array<f64>>();
    private _id: u32 = 0;

    constructor(schema: SchemaDef) {
        this._schema = schema;
        let size = schema.byteSize;
        this._buf = new ArrayBuffer(size > 0 ? size : 1);
        this._view = new DataView(this._buf);
        for (let i = 0; i < schema.fieldCount; i++) {
            this._strings.push("");
            this._arrays.push(new Array<f64>());
        }
    }

    get schemaHash(): u32 { return this._schema.hash; }
    get dirtyMask(): u32 { return this._dirty; }
    get fieldCount(): i32 { return this._schema.fieldCount; }
    get id(): u32 { return this._id; }
    set id(value: u32) { this._id = value; }

    clearDirty(): void { this._dirty = 0; }

    private _bit(index: i32): u32 {
        return (1 as u32) << (index as u32);
    }

    /** Optional fields report false while absent; all others are always present. */
    hasField(index: i32): bool {
        if (!this._schema.isOptional(index)) return true;
        return (this._present & this._bit(index)) != 0;
    }

    /** Clears an optional field; marks the field dirty for the next patch. */
    clearField(index: i32): void {
        if (!this._schema.isOptional(index)) return;
        if ((this._present & this._bit(index)) == 0) return;
        this._present = this._present & ~this._bit(index);
        this._mark(index);
    }

    private _touch(index: i32, changed: bool): void {
        let optional = this._schema.isOptional(index);
        let present = (this._present & this._bit(index)) != 0;
        if (!changed && (!optional || present)) return;
        if (optional) this._present = this._present | this._bit(index);
        this._mark(index);
    }

    getU8(i: i32): u8 { return this._view.getUint8(this._schema.fieldOffset(i)); }
    setU8(i: i32, v: u8): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getUint8(off) != v;
        if (changed) this._view.setUint8(off, v);
        this._touch(i, changed);
    }

    getI8(i: i32): i8 { return this._view.getInt8(this._schema.fieldOffset(i)); }
    setI8(i: i32, v: i8): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getInt8(off) != v;
        if (changed) this._view.setInt8(off, v);
        this._touch(i, changed);
    }

    getU16(i: i32): u16 { return this._view.getUint16(this._schema.fieldOffset(i), true); }
    setU16(i: i32, v: u16): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getUint16(off, true) != v;
        if (changed) this._view.setUint16(off, v, true);
        this._touch(i, changed);
    }

    getI16(i: i32): i16 { return this._view.getInt16(this._schema.fieldOffset(i), true); }
    setI16(i: i32, v: i16): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getInt16(off, true) != v;
        if (changed) this._view.setInt16(off, v, true);
        this._touch(i, changed);
    }

    getU32(i: i32): u32 { return this._view.getUint32(this._schema.fieldOffset(i), true); }
    setU32(i: i32, v: u32): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getUint32(off, true) != v;
        if (changed) this._view.setUint32(off, v, true);
        this._touch(i, changed);
    }

    getI32(i: i32): i32 { return this._view.getInt32(this._schema.fieldOffset(i), true); }
    setI32(i: i32, v: i32): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getInt32(off, true) != v;
        if (changed) this._view.setInt32(off, v, true);
        this._touch(i, changed);
    }

    getF32(i: i32): f32 { return this._view.getFloat32(this._schema.fieldOffset(i), true); }
    setF32(i: i32, v: f32): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getFloat32(off, true) != v;
        if (changed) this._view.setFloat32(off, v, true);
        this._touch(i, changed);
    }

    getF64(i: i32): f64 { return this._view.getFloat64(this._schema.fieldOffset(i), true); }
    setF64(i: i32, v: f64): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getFloat64(off, true) != v;
        if (changed) this._view.setFloat64(off, v, true);
        this._touch(i, changed);
    }

    getBool(i: i32): bool { return this._view.getUint8(this._schema.fieldOffset(i)) != 0; }
    setBool(i: i32, v: bool): void {
        let off = this._schema.fieldOffset(i);
        let byte: u8 = v ? 1 : 0;
        let changed = this._view.getUint8(off) != byte;
        if (changed) this._view.setUint8(off, byte);
        this._touch(i, changed);
    }

    getI64(i: i32): i64 { return this._view.getInt64(this._schema.fieldOffset(i), true); }
    setI64(i: i32, v: i64): void {
        let off = this._schema.fieldOffset(i);
        let changed = this._view.getInt64(off, true) != v;
        if (changed) this._view.setInt64(off, v, true);
        this._touch(i, changed);
    }

    getString(i: i32): string { return this._strings[i]; }
    setString(i: i32, v: string): void {
        let changed = this._strings[i] != v;
        if (changed) this._strings[i] = v;
        this._touch(i, changed);
    }

    getArray(i: i32): Array<f64> { return this._arrays[i]; }
    setArray(i: i32, v: Array<f64>): void {
        let previous = this._arrays[i];
        let changed = !schemaArraysEqual(previous, v);
        if (changed) this._arrays[i] = v;
        this._touch(i, changed);
    }

    getArrayLength(i: i32): i32 { return this._arrays[i].length; }

    private _mark(index: i32): void {
        if (this._schema.isFullOnly(index)) return;
        this._dirty = this._dirty | this._bit(index);
    }

    private writeStringValue(w: DeltaWriter, index: i32): void {
        let bytes = String.UTF8.encode(this._strings[index]);
        let length = bytes.byteLength;
        if (length > 255) length = 255;
        w.writeU8(length as u8);
        if (length > 0) {
            w.writeBytes(length == bytes.byteLength ? bytes : bytes.slice(0, length));
        }
    }

    writeValue(w: DeltaWriter, index: i32): void {
        if (this._schema.isOptional(index)) {
            let present = this.hasField(index);
            w.writeU8(present ? 1 : 0);
            if (!present) return;
        }
        let t = this._schema.fieldType(index);
        if (t == FieldType.STRING) {
            this.writeStringValue(w, index);
            return;
        }
        if (t == FieldType.ARRAY) {
            let element = this._schema.fieldElement(index);
            let values = this._arrays[index];
            let count = values.length;
            if (count > 255) count = 255;
            w.writeU8(count as u8);
            for (let j = 0; j < count; j++) writeSchemaNumber(w, element, values[j]);
            return;
        }
        if (t == FieldType.U8) { w.writeU8(this.getU8(index)); }
        else if (t == FieldType.I8) { w.writeI8(this.getI8(index)); }
        else if (t == FieldType.U16) { w.writeU16(this.getU16(index)); }
        else if (t == FieldType.I16) { w.writeI16(this.getI16(index)); }
        else if (t == FieldType.U32) { w.writeU32(this.getU32(index)); }
        else if (t == FieldType.I32) { w.writeI32(this.getI32(index)); }
        else if (t == FieldType.F32) { w.writeF32(this.getF32(index)); }
        else if (t == FieldType.F64) { w.writeF64(this.getF64(index)); }
        else if (t == FieldType.BOOL) { w.writeBool(this.getBool(index)); }
        else { w.writeI64(this.getI64(index)); }
    }

    writeAll(w: DeltaWriter): void {
        for (let i = 0; i < this._schema.fieldCount; i++) this.writeValue(w, i);
    }

    writeDirty(w: DeltaWriter): void {
        for (let i = 0; i < this._schema.fieldCount; i++) {
            if (this._schema.isFullOnly(i)) continue;
            if ((this._dirty & this._bit(i)) != 0) this.writeValue(w, i);
        }
    }
}

function writeSchemaNumber(w: DeltaWriter, type: u8, value: f64): void {
    if (type == FieldType.U8) { w.writeU8(value as u8); }
    else if (type == FieldType.I8) { w.writeI8(value as i8); }
    else if (type == FieldType.U16) { w.writeU16(value as u16); }
    else if (type == FieldType.I16) { w.writeI16(value as i16); }
    else if (type == FieldType.U32) { w.writeU32(value as u32); }
    else if (type == FieldType.I32) { w.writeI32(value as i32); }
    else if (type == FieldType.F32) { w.writeF32(value as f32); }
    else if (type == FieldType.BOOL) { w.writeBool(value != 0); }
    else if (type == FieldType.I64) { w.writeI64(value as i64); }
    else { w.writeF64(value); }
}

/** Base for schema states: subclasses override commit() with encoded delta. */
export class SchemaStateBase {
    commit(): i64 { return 0; }
}

/**
 * A map of `id -> SchemaEntity` that emits full snapshots and field patches.
 *
 * ```ts
 * const state = new SchemaState(PlayerSchema);
 * state.set(id, player);        // add or replace
 * state.delete(id);             // remove
 * return state.seal();          // packed pointer for onTick
 * ```
 */
export class SchemaState extends SchemaStateBase {
    private _schema: SchemaDef;
    private _entities: Map<u32, SchemaEntity> = new Map<u32, SchemaEntity>();
    private _order: Array<u32> = new Array<u32>();
    private _added: Array<u32> = new Array<u32>();
    private _removed: Array<u32> = new Array<u32>();
    private _full: DeltaWriter = new DeltaWriter(512);
    private _patch: DeltaWriter = new DeltaWriter(512);
    private _out: DeltaWriter = new DeltaWriter(1024);

    constructor(schema: SchemaDef) {
        super();
        this._schema = schema;
    }

    get schemaHash(): u32 { return this._schema.hash; }
    get size(): i32 { return this._order.length; }
    get ids(): Array<u32> { return this._order; }
    keys(): Array<u32> { return this._order; }
    values(): Array<SchemaEntity> { return this._entities.values(); }

    has(id: u32): bool { return this._entities.has(id); }
    get(id: u32): SchemaEntity { return this._entities.get(id); }

    set(id: u32, entity: SchemaEntity): void {
        if (entity.schemaHash != this._schema.hash) return;
        let removedAt = this._removed.indexOf(id);
        if (removedAt >= 0) this._removed.splice(removedAt, 1);
        if (!this._entities.has(id)) {
            this._order.push(id);
            this._added.push(id);
        }
        entity.id = id;
        this._entities.set(id, entity);
    }

    delete(id: u32): void {
        if (!this._entities.has(id)) return;
        this._entities.delete(id);
        let orderAt = this._order.indexOf(id);
        if (orderAt >= 0) this._order.splice(orderAt, 1);
        let addedAt = this._added.indexOf(id);
        if (addedAt >= 0) {
            this._added.splice(addedAt, 1);
        } else {
            this._removed.push(id);
        }
    }

    clear(): void {
        this._entities.clear();
        this._order.length = 0;
        this._added.length = 0;
        this._removed.length = 0;
    }

    /** Append a complete single-collection full section (kind 1). */
    writeFull(w: DeltaWriter): void {
        writeSchemaMagic(w);
        w.writeU8(1);
        w.writeU32(this._schema.hash);
        w.writeU32(this._order.length as u32);
        for (let i = 0; i < this._order.length; i++) {
            let id = this._order[i];
            let e = this._entities.get(id);
            w.writeU32(id);
            e.writeAll(w);
        }
    }

    /** Number of operations the next patch section will carry. */
    patchOpCount(): i32 {
        let updates = 0;
        for (let i = 0; i < this._order.length; i++) {
            let id = this._order[i];
            let e = this._entities.get(id);
            if (e.dirtyMask != 0 && this._added.indexOf(id) < 0) updates++;
        }
        return this._removed.length + this._added.length + updates;
    }

    /** Append a single-collection patch section (kind 2), even when empty. */
    writePatch(w: DeltaWriter): void {
        writeSchemaMagic(w);
        w.writeU8(2);
        w.writeU32(this._schema.hash);
        w.writeU32(this.patchOpCount() as u32);

        for (let i = 0; i < this._removed.length; i++) {
            w.writeU8(2); // REMOVE
            w.writeU32(this._removed[i]);
        }
        for (let i = 0; i < this._added.length; i++) {
            let id = this._added[i];
            let e = this._entities.get(id);
            w.writeU8(1); // ADD
            w.writeU32(id);
            e.writeAll(w);
        }
        for (let i = 0; i < this._order.length; i++) {
            let id = this._order[i];
            if (this._added.indexOf(id) >= 0) continue;
            let e = this._entities.get(id);
            let mask = e.dirtyMask;
            if (mask == 0) continue;
            w.writeU8(3); // UPDATE
            w.writeU32(id);
            w.writeU32(mask);
            e.writeDirty(w);
        }
    }

    private encodeFull(): void {
        this._full.reset();
        this.writeFull(this._full);
    }

    private encodePatch(): void {
        this._patch.reset();
        this.writePatch(this._patch);
    }

    /** Clear per-tick dirty flags and queued adds/removes. */
    clearChanges(): void {
        for (let i = 0; i < this._order.length; i++) {
            this._entities.get(this._order[i]).clearDirty();
        }
        this._added.length = 0;
        this._removed.length = 0;
    }

    /**
     * Encode the current full snapshot and patch into one reusable buffer and
     * return the packed `(ptr << 32) | len` for `onTick`.
     */
    seal(): i64 {
        this.encodeFull();
        this.encodePatch();

        let out = this._out;
        out.reset();
        writeSchemaMagic(out);
        out.writeU32(this._full.length as u32);
        out.writeWriter(this._full);
        out.writeU32(this._patch.length as u32);
        out.writeWriter(this._patch);

        this.clearChanges();
        return out.seal();
    }

    override commit(): i64 {
        return this.seal();
    }
}

/**
 * Multi-collection world: root scalar fields plus any number of
 * `SchemaState` maps, sealed into one full/patch pair.
 *
 * ```ts
 * const world = new GameState();      // generated by `ox-game schema`
 * world.players.set(id, player);
 * world.tick = tick;
 * return world.seal();
 * ```
 *
 * Wire (inside the same envelope as `SchemaState`):
 *   multi full:  "OXS2" [u8 3] [u32 root hash] [u16 root fields] [values...]
 *                [u16 collection count] { [u16 id] [u32 hash] [u32 len] [section] }*
 *   multi patch: "OXS2" [u8 4] [u32 root hash] [u16 root fields]
 *                [u32 root dirty mask] [dirty values...]
 *                [u16 collection count] { [u16 id] [u32 hash] [u32 len] [section] }*
 */
export class SchemaWorld extends SchemaStateBase {
    root: SchemaEntity;
    private _collections: Array<SchemaState> = new Array<SchemaState>();
    private _full: DeltaWriter = new DeltaWriter(1024);
    private _patch: DeltaWriter = new DeltaWriter(1024);
    private _section: DeltaWriter = new DeltaWriter(512);
    private _out: DeltaWriter = new DeltaWriter(2048);

    constructor(root: SchemaEntity) {
        super();
        this.root = root;
    }

    /** Register a collection; registration order is the wire collection id. */
    add(state: SchemaState): this {
        this._collections.push(state);
        return this;
    }

    get collectionCount(): i32 { return this._collections.length; }

    private encodeFull(): void {
        let w = this._full;
        w.reset();
        writeSchemaMagic(w);
        w.writeU8(3);
        w.writeU32(this.root.schemaHash);
        w.writeU16(this.root.fieldCount as u16);
        this.root.writeAll(w);
        w.writeU16(this._collections.length as u16);
        for (let i = 0; i < this._collections.length; i++) {
            let state = this._collections[i];
            this._section.reset();
            state.writeFull(this._section);
            w.writeU16(i as u16);
            w.writeU32(state.schemaHash);
            w.writeU32(this._section.length as u32);
            w.writeWriter(this._section);
        }
    }

    private encodePatch(): void {
        let w = this._patch;
        w.reset();
        writeSchemaMagic(w);
        w.writeU8(4);
        w.writeU32(this.root.schemaHash);
        w.writeU16(this.root.fieldCount as u16);
        w.writeU32(this.root.dirtyMask);
        this.root.writeDirty(w);

        let changed = 0;
        for (let i = 0; i < this._collections.length; i++) {
            if (this._collections[i].patchOpCount() > 0) changed++;
        }
        w.writeU16(changed as u16);
        for (let i = 0; i < this._collections.length; i++) {
            let state = this._collections[i];
            if (state.patchOpCount() == 0) continue;
            this._section.reset();
            state.writePatch(this._section);
            w.writeU16(i as u16);
            w.writeU32(state.schemaHash);
            w.writeU32(this._section.length as u32);
            w.writeWriter(this._section);
        }
    }

    /** Encode full + patch sections and return the packed pointer/len. */
    seal(): i64 {
        this.encodeFull();
        this.encodePatch();

        let out = this._out;
        out.reset();
        writeSchemaMagic(out);
        out.writeU32(this._full.length as u32);
        out.writeWriter(this._full);
        out.writeU32(this._patch.length as u32);
        out.writeWriter(this._patch);

        this.root.clearDirty();
        for (let i = 0; i < this._collections.length; i++) {
            this._collections[i].clearChanges();
        }
        return out.seal();
    }

    override commit(): i64 {
        return this.seal();
    }
}

// =============================================================================
// --- 5.5 UNIVERSAL HARDWARE KEYCODES (A-Z, 0-9, ARROWS, F1-F12, MOUSE) ---
// =============================================================================

export enum KeyCode {
    None            = 0,

    // Mouse Buttons
    MouseLeft       = 1,
    MouseMiddle     = 2,
    MouseRight      = 3,
    MouseBack       = 4,
    MouseForward    = 5,

    // Control & Whitespace
    Backspace       = 8,
    Tab             = 9,
    Enter           = 13,
    Shift           = 16,
    Control         = 17,
    Alt             = 18,
    Pause           = 19,
    CapsLock        = 20,
    Escape          = 27,
    Space           = 32,

    // Navigation & Page Keys
    PageUp          = 33,
    PageDown        = 34,
    End             = 35,
    Home            = 36,
    Insert          = 45,
    Delete          = 46,
    PrintScreen     = 44,

    // Arrow Navigation Keys
    ArrowLeft       = 37,
    ArrowUp         = 38,
    ArrowRight      = 39,
    ArrowDown       = 40,
    Left            = 37,
    Up              = 38,
    Right           = 39,
    Down            = 40,

    // Number Keys (0 - 9) Top Row
    Digit0          = 48,
    Digit1          = 49,
    Digit2          = 50,
    Digit3          = 51,
    Digit4          = 52,
    Digit5          = 53,
    Digit6          = 54,
    Digit7          = 55,
    Digit8          = 56,
    Digit9          = 57,

    // Digit Aliases
    Num0            = 48,
    Num1            = 49,
    Num2            = 50,
    Num3            = 51,
    Num4            = 52,
    Num5            = 53,
    Num6            = 54,
    Num7            = 55,
    Num8            = 56,
    Num9            = 57,

    // Alphabet Keys (A - Z)
    KeyA            = 65,
    KeyB            = 66,
    KeyC            = 67,
    KeyD            = 68,
    KeyE            = 69,
    KeyF            = 70,
    KeyG            = 71,
    KeyH            = 72,
    KeyI            = 73,
    KeyJ            = 74,
    KeyK            = 75,
    KeyL            = 76,
    KeyM            = 77,
    KeyN            = 78,
    KeyO            = 79,
    KeyP            = 80,
    KeyQ            = 81,
    KeyR            = 82,
    KeyS            = 83,
    KeyT            = 84,
    KeyU            = 85,
    KeyV            = 86,
    KeyW            = 87,
    KeyX            = 88,
    KeyY            = 89,
    KeyZ            = 90,

    // Single-Letter Aliases
    A               = 65,
    B               = 66,
    C               = 67,
    D               = 68,
    E               = 69,
    F               = 70,
    G               = 71,
    H               = 72,
    I               = 73,
    J               = 74,
    K               = 75,
    L               = 76,
    M               = 77,
    N               = 78,
    O               = 79,
    P               = 80,
    Q               = 81,
    R               = 82,
    S               = 83,
    T               = 84,
    U               = 85,
    V               = 86,
    W               = 87,
    X               = 88,
    Y               = 89,
    Z               = 90,

    // System & Meta Keys
    MetaLeft        = 91,
    MetaRight       = 92,
    ContextMenu     = 93,

    // Numpad Keys (0 - 9 and Math Operators)
    Numpad0         = 96,
    Numpad1         = 97,
    Numpad2         = 98,
    Numpad3         = 99,
    Numpad4         = 100,
    Numpad5         = 101,
    Numpad6         = 102,
    Numpad7         = 103,
    Numpad8         = 104,
    Numpad9         = 105,
    NumpadMultiply  = 106,
    NumpadAdd       = 107,
    NumpadSeparator = 108,
    NumpadSubtract  = 109,
    NumpadDecimal   = 110,
    NumpadDivide    = 111,

    // Function Keys (F1 - F12)
    F1              = 112,
    F2              = 113,
    F3              = 114,
    F4              = 115,
    F5              = 116,
    F6              = 117,
    F7              = 118,
    F8              = 119,
    F9              = 120,
    F10             = 121,
    F11             = 122,
    F12             = 123,

    // Extended Function Keys (F13 - F24)
    F13             = 124,
    F14             = 125,
    F15             = 126,
    F16             = 127,
    F17             = 128,
    F18             = 129,
    F19             = 130,
    F20             = 131,
    F21             = 132,
    F22             = 133,
    F23             = 134,
    F24             = 135,

    // Lock Keys
    NumLock         = 144,
    ScrollLock      = 145,

    // Modifiers Specific
    ShiftLeft       = 160,
    ShiftRight      = 161,
    ControlLeft     = 162,
    ControlRight    = 163,
    AltLeft         = 164,
    AltRight        = 165,

    // Punctuation & Symbols
    Semicolon       = 186,
    Equal           = 187,
    Comma           = 188,
    Minus           = 189,
    Period          = 190,
    Slash           = 191,
    Backquote       = 192,
    BracketLeft     = 219,
    Backslash       = 220,
    BracketRight    = 221,
    Quote           = 222
}

export class KeyCodeHelper {
    static isLetter(key: KeyCode): bool {
        return key >= KeyCode.KeyA && key <= KeyCode.KeyZ;
    }
    static isDigit(key: KeyCode): bool {
        return (key >= KeyCode.Digit0 && key <= KeyCode.Digit9) ||
               (key >= KeyCode.Numpad0 && key <= KeyCode.Numpad9);
    }
    static isArrow(key: KeyCode): bool {
        return key >= KeyCode.ArrowLeft && key <= KeyCode.ArrowDown;
    }
    static isFunctionKey(key: KeyCode): bool {
        return key >= KeyCode.F1 && key <= KeyCode.F12;
    }
    static isModifier(key: KeyCode): bool {
        return key == KeyCode.Shift || key == KeyCode.Control || key == KeyCode.Alt ||
               key == KeyCode.ShiftLeft || key == KeyCode.ShiftRight ||
               key == KeyCode.ControlLeft || key == KeyCode.ControlRight ||
               key == KeyCode.AltLeft || key == KeyCode.AltRight;
    }
    static isMouseButton(key: KeyCode): bool {
        return key >= KeyCode.MouseLeft && key <= KeyCode.MouseForward;
    }
}

// =============================================================================
// --- 6. TYPE-SAFE CLIENT INPUT READER (NO RAW POINTERS IN ROOM) ---
// =============================================================================

export class InputReader {
    private _ptr: usize;
    private _len: i32;
    private _offset: i32 = 0;



    constructor(ptr: usize, len: i32) {
        this._ptr = ptr;
        this._len = len;
        this._offset = 0;
    }

    get remaining(): i32 {
        return this._len - this._offset;
    }

    hasRemaining(bytes: i32): bool {
        return (this._len - this._offset) >= bytes;
    }

    readU8(): u8 {
        if (this._offset + 1 > this._len) return 0;
        let val = load<u8>(this._ptr + this._offset);
        this._offset += 1;
        return val;
    }

    readU32(): u32 {
        if (this._offset + 4 > this._len) return 0;
        let val = load<u32>(this._ptr + this._offset);
        this._offset += 4;
        return val;
    }

    readF32(): f32 {
        if (this._offset + 4 > this._len) return 0.0;
        let val = load<f32>(this._ptr + this._offset);
        this._offset += 4;
        return val;
    }

    readF64(): f64 {
        if (this._offset + 8 > this._len) return 0.0;
        let val = load<f64>(this._ptr + this._offset);
        this._offset += 8;
        return val;
    }

    readI64(): i64 {
        if (this._offset + 8 > this._len) return 0;
        let val = load<i64>(this._ptr + this._offset);
        this._offset += 8;
        return val;
    }

    /** Reads a 2D vector (joystick / WASD movement) */
    readVec2(): Vec2 {
        let x = this.readF32() as f64;
        let y = this.readF32() as f64;
        return new Vec2(x, y);
    }

    /** Reads a 3D vector (camera aim / direction) */
    readVec3(): Vec3 {
        let x = this.readF32() as f64;
        let y = this.readF32() as f64;
        let z = this.readF32() as f64;
        return new Vec3(x, y, z);
    }

    readU16(): u16 {
        if (this._offset + 2 > this._len) return 0;
        let val = load<u16>(this._ptr + this._offset);
        this._offset += 2;
        return val;
    }

    readI32(): i32 {
        if (this._offset + 4 > this._len) return 0;
        let val = load<i32>(this._ptr + this._offset);
        this._offset += 4;
        return val;
    }
}


// =============================================================================
// --- 7. DETERMINISTIC PRNG (REPRODUCIBLE RANDOM GENERATOR) ---
// =============================================================================

/**
 * Fast Deterministic 64-bit XorShift PRNG.
 * Ensures identical random sequences across client prediction, server rewinds, and match replays!
 */
export class Random {
    private static _state: u64 = 88172645463325252;

    static seed(s: u64): void {
        this._state = (s == 0) ? 88172645463325252 : s;
    }

    static nextU32(): u32 {
        let x = this._state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        this._state = x;
        return (x & 0xFFFFFFFF) as u32;
    }

    static nextFloat(): f64 {
        return (this.nextU32() as f64) / 4294967296.0;
    }

    static range(min: i32, max: i32): i32 {
        if (min >= max) return min;
        let span = (max - min) as f64;
        return min + ((this.nextFloat() * span) as i32);
    }

    static rangeFloat(min: f64, max: f64): f64 {
        return min + this.nextFloat() * (max - min);
    }

    /**
     * Chance check: Returns true if roll <= percentage (0.0 to 100.0).
     * Example: Random.chance(25.0) -> 25% crit chance.
     */
    static chance(percentage: f64): bool {
        return (this.nextFloat() * 100.0) <= percentage;
    }
}

// =============================================================================
// --- 8. COOLDOWN & TIMER MANAGER ---
// =============================================================================

/**
 * Cooldown tracker for weapons, abilities, dashes, and respawns.
 * Zero-GC: updates with tick delta time!
 */
export class CooldownTracker {
    private _cooldowns: Map<string, f64> = new Map<string, f64>();

    update(dt: f64): void {
        let keys = this._cooldowns.keys();
        for (let i = 0; i < keys.length; i++) {
            let k = keys[i];
            let rem = this._cooldowns.get(k) - dt;
            if (rem <= 0.0) {
                this._cooldowns.delete(k);
            } else {
                this._cooldowns.set(k, rem);
            }
        }
    }

    isReady(actionKey: string): bool {
        return !this._cooldowns.has(actionKey);
    }

    trigger(actionKey: string, durationSeconds: f64): void {
        this._cooldowns.set(actionKey, durationSeconds);
    }

    getRemaining(actionKey: string): f64 {
        if (!this._cooldowns.has(actionKey)) return 0.0;
        return this._cooldowns.get(actionKey);
    }

    reset(): void {
        this._cooldowns.clear();
    }
}

// =============================================================================
// --- 9. TRIGGER VOLUMES & INTERACTIVE ZONES ---
// =============================================================================

export enum ZoneType {
    NONE = 0,
    DAMAGE = 1,       // Lava, poison gas, storm circle
    SPEED_BUFF = 2,   // Speed track, booster pads
    SPEED_SLOW = 3,   // Water, mud, web
    JUMP_PAD = 4,     // Launches player vertically
    CAPTURE_ZONE = 5, // King of the Hill / Domination flag
    TELEPORT = 6      // Instant portal
}

export class TriggerZone {
    id: u32;
    type: ZoneType;
    minX: f64; minY: f64; minZ: f64;
    maxX: f64; maxY: f64; maxZ: f64;
    value: f64; // Damage per second, speed multiplier, or jump impulse force

    constructor(id: u32, type: ZoneType, minX: f64, minY: f64, minZ: f64, maxX: f64, maxY: f64, maxZ: f64, value: f64 = 0.0) {
        this.id = id;
        this.type = type;
        this.minX = minX; this.minY = minY; this.minZ = minZ;
        this.maxX = maxX; this.maxY = maxY; this.maxZ = maxZ;
        this.value = value;
    }

    contains(pos: Vec3): bool {
        return this.containsRaw(pos.x, pos.y, pos.z);
    }

    containsRaw(px: f64, py: f64, pz: f64): bool {
        return px >= this.minX && px <= this.maxX &&
               py >= this.minY && py <= this.maxY &&
               pz >= this.minZ && pz <= this.maxZ;
    }
}

// =============================================================================
// --- 10. COMBAT & DAMAGE RESOLUTION ---
// =============================================================================

export enum HitZone {
    BODY = 0,
    HEAD = 1,
    LIMB = 2
}

export class DamageResult {
    damageDealt: f64 = 0.0;
    shieldAbsorbed: f64 = 0.0;
    healthRemaining: f64 = 0.0;
    isKilled: bool = false;
    isCritical: bool = false;
}

export class Combat {
    private static _res: DamageResult = new DamageResult();

    /**
     * Calculates combat damage with headshot multipliers, armor mitigation, and shield absorption.
     * Zero-GC pre-allocated return!
     */
    static applyDamage(
        baseDamage: f64,
        hitZone: HitZone,
        armor: f64,
        currentShield: f64,
        currentHealth: f64
    ): DamageResult {
        let res = Combat._res;
        let mult: f64 = 1.0;
        let isCrit = false;

        if (hitZone == HitZone.HEAD) {
            mult = 2.0; // 2x Headshot critical multiplier
            isCrit = true;
        } else if (hitZone == HitZone.LIMB) {
            mult = 0.75;
        }

        let raw = baseDamage * mult;
        let effective = raw * (100.0 / (100.0 + Math.max(0.0, armor)));
        let shieldAbsorb = Math.min(currentShield, effective);
        let healthDamage = effective - shieldAbsorb;
        let remHealth = Math.max(0.0, currentHealth - healthDamage);

        res.damageDealt = effective;
        res.shieldAbsorbed = shieldAbsorb;
        res.healthRemaining = remHealth;
        res.isKilled = remHealth <= 0.001;
        res.isCritical = isCrit;
        return res;
    }
}

// =============================================================================
// --- 11. MATCH LIFECYCLE & STATE MACHINE ---
// =============================================================================

export enum MatchPhase {
    WAITING_FOR_PLAYERS = 0,
    COUNTDOWN = 1,
    ROUND_ACTIVE = 2,
    OVERTIME = 3,
    ROUND_OVER = 4,
    RESETTING = 5
}

export class MatchStateMachine {
    phase: MatchPhase = MatchPhase.WAITING_FOR_PLAYERS;
    timer: f64 = 0.0;
    roundNumber: i32 = 1;

    startCountdown(durationSeconds: f64 = 5.0): void {
        this.phase = MatchPhase.COUNTDOWN;
        this.timer = durationSeconds;
    }

    startRound(roundDurationSeconds: f64 = 300.0): void {
        this.phase = MatchPhase.ROUND_ACTIVE;
        this.timer = roundDurationSeconds;
    }

    endRound(postRoundSeconds: f64 = 10.0): void {
        this.phase = MatchPhase.ROUND_OVER;
        this.timer = postRoundSeconds;
    }

    update(dt: f64): bool {
        if (this.timer > 0.0) {
            this.timer -= dt;
            if (this.timer <= 0.0) {
                this.timer = 0.0;
                return true; // Phase timer just expired!
            }
        }
        return false;
    }
}

// =============================================================================
// --- 12. FAST PROXIMITY SPATIAL GRID (AOI) ---
// =============================================================================

export class SpatialGrid2D {
    private entityX: Map<i64, f64> = new Map<i64, f64>();
    private entityZ: Map<i64, f64> = new Map<i64, f64>();

    update(id: i64, pos: Vec3): void {
        this.updateRaw(id, pos.x, pos.z);
    }

    updateRaw(id: i64, x: f64, z: f64): void {
        this.entityX.set(id, x);
        this.entityZ.set(id, z);
    }

    remove(id: i64): void {
        this.entityX.delete(id);
        this.entityZ.delete(id);
    }

    queryRadius(center: Vec3, radius: f64): StaticArray<i64> {
        return this.queryRadiusRaw(center.x, center.z, radius);
    }

    queryRadiusRaw(cx: f64, cz: f64, radius: f64): StaticArray<i64> {
        let rSq = radius * radius;
        let ids = this.entityX.keys();
        let count = 0;

        for (let i = 0; i < ids.length; i++) {
            let id = ids[i];
            let dx = this.entityX.get(id) - cx;
            let dz = this.entityZ.get(id) - cz;
            if (dx * dx + dz * dz <= rSq) {
                count++;
            }
        }

        let res = new StaticArray<i64>(count);
        let idx = 0;
        for (let i = 0; i < ids.length; i++) {
            let id = ids[i];
            let dx = this.entityX.get(id) - cx;
            let dz = this.entityZ.get(id) - cz;
            if (dx * dx + dz * dz <= rSq) {
                res[idx++] = id;
            }
        }
        return res;
    }
}

// =============================================================================
// --- 13. BOT & NPC STEERING BEHAVIORS ---
// =============================================================================

export class Steering {
    static seek(from: Vec3, target: Vec3, maxSpeed: f64): Vec3 {
        return Steering.seekRaw(from.x, from.z, target.x, target.z, maxSpeed);
    }

    static seekRaw(fromX: f64, fromZ: f64, targetX: f64, targetZ: f64, maxSpeed: f64): Vec3 {
        let dx = targetX - fromX;
        let dz = targetZ - fromZ;
        let dist = Math.sqrt(dx * dx + dz * dz);
        if (dist > 0.001) {
            return new Vec3((dx / dist) * maxSpeed, 0.0, (dz / dist) * maxSpeed);
        }
        return new Vec3(0, 0, 0);
    }

    static flee(from: Vec3, threat: Vec3, maxSpeed: f64): Vec3 {
        return Steering.fleeRaw(from.x, from.z, threat.x, threat.z, maxSpeed);
    }

    static fleeRaw(fromX: f64, fromZ: f64, threatX: f64, threatZ: f64, maxSpeed: f64): Vec3 {
        let dx = fromX - threatX;
        let dz = fromZ - threatZ;
        let dist = Math.sqrt(dx * dx + dz * dz);
        if (dist > 0.001) {
            return new Vec3((dx / dist) * maxSpeed, 0.0, (dz / dist) * maxSpeed);
        }
        return new Vec3(0, 0, 0);
    }

    static arrive(from: Vec3, target: Vec3, maxSpeed: f64, slowingDistance: f64): Vec3 {
        return Steering.arriveRaw(from.x, from.z, target.x, target.z, maxSpeed, slowingDistance);
    }

    static arriveRaw(fromX: f64, fromZ: f64, targetX: f64, targetZ: f64, maxSpeed: f64, slowingDistance: f64): Vec3 {
        let dx = targetX - fromX;
        let dz = targetZ - fromZ;
        let dist = Math.sqrt(dx * dx + dz * dz);
        if (dist > 0.001) {
            let speed = maxSpeed;
            if (dist < slowingDistance) {
                speed = maxSpeed * (dist / slowingDistance);
            }
            return new Vec3((dx / dist) * speed, 0.0, (dz / dist) * speed);
        }
        return new Vec3(0, 0, 0);
    }
}



// =============================================================================
// --- 15. ABSTRACT ROOM BASE CLASS ---
// =============================================================================

/// `room` is injected by the SDK so callbacks never have to capture `this`
/// (AssemblyScript's no-GC `stub` runtime cannot allocate closure environments).
export type MessageHandlerCallback = (senderId: i64, reader: InputReader, room: Room) => void;
export type RoomEventCallback = (id: i64, val0: f64, val1: f64, room: Room | null) => void;

/// OpenOxide typed app-message opcode. This is our own wire envelope, not
/// Colyseus/msgpack: [0x05][type_len u8][UTF-8 type][opaque payload].
const APP_MESSAGE_OPCODE: u8 = 0x05;
const APP_MESSAGE_MAX_TYPE: i32 = 255;

/**
 * Universal Zero-Overhead Event Emitter for OpenOxide game rooms.
 * Built into the SDK so developers do not need manual event plumbing.
 */
export class EventEmitter {
    private _handlers: Map<string, Array<RoomEventCallback>> = new Map<string, Array<RoomEventCallback>>();
    private _owner: Room | null;

    constructor(owner: Room | null = null) {
        this._owner = owner;
    }

    /** Subscribes a listener callback to a named event */
    on(event: string, callback: RoomEventCallback): void {
        let list: Array<RoomEventCallback>;
        if (this._handlers.has(event)) {
            list = this._handlers.get(event);
        } else {
            list = new Array<RoomEventCallback>();
            this._handlers.set(event, list);
        }
        list.push(callback);
    }

    /** Emits a named event to all subscribed listeners */
    emit(event: string, id: i64 = 0, val0: f64 = 0.0, val1: f64 = 0.0): void {
        if (this._handlers.has(event)) {
            let list = this._handlers.get(event);
            for (let i = 0; i < list.length; i++) {
                list[i](id, val0, val1, this._owner);
            }
        }
    }

    /** Clears all listeners for an event or all events */
    clear(): void {
        this._handlers = new Map<string, Array<RoomEventCallback>>();
    }
}

/**
 * Authoritative Game Room Base Class for AssemblyScript
 * Extend this class and implement the abstract lifecycle hooks.
 * The OpenOxide compiler automatically creates the Flat WASM ABI bridge!
 */
export abstract class Room {
    maxPlayers: i32 = 16;

    /// Built-in SDK Event Emitter (Zero manual plumbing required by backend)
    events: EventEmitter = new EventEmitter(this);
    /// Typed app messages from clients, keyed by UTF-8 message name.
    private _textMessageHandlers: Map<string, MessageHandlerCallback> = new Map<string, MessageHandlerCallback>();
    /// Legacy numeric-opcode routing for raw binary clients.
    private _messageHandlers: Map<u8, MessageHandlerCallback> = new Map<u8, MessageHandlerCallback>();

    /// Registers a handler for a named client message:
    /// `this.onMessage("discard", (clientId, reader, room) => { ... })`.
    onMessage(type: string, handler: MessageHandlerCallback): void {
        this._textMessageHandlers.set(type, handler);
    }

    /// Legacy numeric-opcode handler for existing raw-binary clients.
    onOpcodeMessage(opcode: u8, handler: MessageHandlerCallback): void {
        this._messageHandlers.set(opcode, handler);
    }

    /// Broadcast a typed message to every player in the room.
    /// `payload` is opaque bytes; use a DeltaWriter/ArrayBuffer or `broadcastText`.
    broadcast(type: string, payload: ArrayBuffer | null = null): bool {
        return this._sendMessage(0, type, payload);
    }

    /// Send a typed message to one player id (the value passed to `onJoin`).
    sendTo(playerId: i64, type: string, payload: ArrayBuffer | null = null): bool {
        return this._sendMessage(playerId, type, payload);
    }

    /// Convenience overload for UTF-8 text payloads.
    broadcastText(type: string, text: string): bool {
        return this._sendMessage(0, type, String.UTF8.encode(text));
    }

    /// Convenience overload for a UTF-8 text payload to one player.
    sendTextTo(playerId: i64, type: string, text: string): bool {
        return this._sendMessage(playerId, type, String.UTF8.encode(text));
    }

    private _sendMessage(target: i64, type: string, payload: ArrayBuffer | null): bool {
        let typeBytes = String.UTF8.encode(type);
        if (typeBytes.byteLength == 0 || typeBytes.byteLength > APP_MESSAGE_MAX_TYPE) {
            return false;
        }
        let data: ArrayBuffer;
        if (payload == null) {
            data = new ArrayBuffer(0);
        } else {
            data = payload;
        }
        let rc = _host_send(
            target,
            changetype<usize>(typeBytes),
            typeBytes.byteLength,
            changetype<usize>(data),
            data.byteLength
        );
        return rc == 0;
    }

    /// Subscribes a callback to a named room event.
    on(event: string, callback: RoomEventCallback): void {
        this.events.on(event, callback);
    }

    /// Emits a room event to all registered listeners.
    emit(event: string, id: i64 = 0, val0: f64 = 0.0, val1: f64 = 0.0): void {
        this.events.emit(event, id, val0, val1);
    }

    /// Called once when the room instance is allocated on the host.
    abstract onCreate(max: i32): void;

    /// Called when a new player joins the room.
    abstract onJoin(id: i64, spawnX: f64, spawnZ: f64): void;

    /// Called when a player leaves or disconnects.
    abstract onLeave(id: i64): void;

    /// Called when an authoritative client input packet arrives.
    /// Typed app messages are routed to `onMessage`; legacy opcodes fall through to
    /// registered `onOpcodeMessage` handlers or `onInputPacket`.
    onInput(id: i64, ptr: usize, len: i32): void {
        if (len <= 0) return;

        // OpenOxide typed app message: [0x05][type_len u8][type][payload].
        if (len >= 2 && load<u8>(ptr) == APP_MESSAGE_OPCODE) {
            let typeLen = load<u8>(ptr + 1) as i32;
            if (typeLen > 0 && 2 + typeLen <= len) {
                let type = String.UTF8.decodeUnsafe(ptr + 2, typeLen);
                if (this._textMessageHandlers.has(type)) {
                    let handler = this._textMessageHandlers.get(type);
                    handler(id, new InputReader(ptr + 2 + typeLen, len - 2 - typeLen), this);
                    return;
                }
                this.onInputPacket(id, new InputReader(ptr + 2 + typeLen, len - 2 - typeLen));
                return;
            }
        }

        let reader = new InputReader(ptr, len);
        if (len != 20 && this._messageHandlers.size > 0) {
            let opcode = load<u8>(ptr);
            if (this._messageHandlers.has(opcode)) {
                let handler = this._messageHandlers.get(opcode);
                handler(id, reader, this);
                return;
            }
        }
        this.onInputPacket(id, reader);
    }

    /// High-level client input packet hook. Zero raw pointers or offsets!
    onInputPacket(id: i64, input: InputReader): void {
        // Override this in your game room
    }

    /// Fixed simulation tick (20 Hz - 128 Hz).
    /// Return packed (ptr << 32) | len for the serialized delta buffer (e.g. deltaWriter.seal()).
    abstract onTick(dt: f64): i64;

    /// State persistence snapshot hook (optional).
    onSave(): i64 {
        return 0;
    }

    /// State restoration hook from saved linear snapshot (optional).
    /// By default, wraps the pointer in an InputReader and dispatches to onLoadSnapshot.
    onLoad(ptr: usize, len: i32): void {
        this.onLoadSnapshot(new InputReader(ptr, len));
    }

    /// High-level state restoration hook (Zero raw pointers!)
    onLoadSnapshot(reader: InputReader): void {
        // Override this in your game room
    }

    /// Guest memory allocator for inbound client network packets.
    onAlloc(len: i32): i32 {
        return changetype<usize>(new ArrayBuffer(len)) as i32;
    }

    /// Room fullness check.
    onIsFull(count: i32): bool {
        return count >= this.maxPlayers;
    }

    /// Minimum milliseconds between state patches; 0 broadcasts every
    /// simulation tick (the backwards-compatible default). When a patch is
    /// skipped the next sent frame carries a full snapshot, so no change is
    /// ever lost.
    patchRate: i32 = 0;

    /// Minimum milliseconds between simulation ticks; 0 runs every host tick.
    /// The host calls `onTick` with a fixed `dt` of `simulationInterval / 1000`.
    simulationInterval: i32 = 0;

    getPatchRate(): i32 {
        return this.patchRate;
    }

    getSimulationRate(): i32 {
        return this.simulationInterval;
    }

    // --- Colyseus-parity sugar ---------------------------------------------

    /// Wire the generated schema state (`ox-game schema-codegen --lang as`):
    /// `this.setState(new State())` then `onTick` returns `this.commitState()`.
    /// Only dirty rows are encoded — developer never touches bytes.
    /// Non-null by default: setState() swaps in the generated state; without
    /// one, commitState() emits nothing.
    protected schemaState: SchemaStateBase = new SchemaStateBase();

    setState(state: SchemaStateBase): void {
        this.schemaState = state;
    }

    /** Call at the end of onTick: `return this.commitState();` */
    commitState(): i64 {
        return this.schemaState.commit();
    }

    setPatchRate(ms: i32): void {
        this.patchRate = ms;
    }

    setSimulationInterval(intervalMs: i32): void {
        this.simulationInterval = intervalMs;
    }

    /// Seat admission. `options` is the raw JSON string the client passed to
    /// `joinOrCreate(room, options)` (empty when it sent none). Return `false`
    /// to reject the connection before any player id or state is created.
    onAuth(clientId: i64, options: string): bool {
        return true;
    }

    /// Called when a disconnected client resumes its reserved seat. The player
    /// id and guest state are unchanged; `onJoin` is NOT called again.
    onReconnect(clientId: i64): void {
        // Override to restore per-client UI state
    }

    /// Called once when the host disposes this room (empty timeout / shutdown).
    onDispose(): void {
        // Override to flush scores, release external resources, ...
    }

    /// Milliseconds a disconnected client may take to reconnect before
    /// `onLeave` runs. 0 (default) releases the seat immediately.
    allowReconnection(clientId: i64): i32 {
        return 0;
    }

    /// Runs immediately before `onTick` (simulation + serialization).
    onBeforePatch(dt: f64): void {
        // Override for pre-serialization bookkeeping
    }

    /// Runs immediately after `onTick` returned its delta.
    onAfterPatch(dt: f64): void {
        // Override for post-serialization bookkeeping
    }

    /// Pack linear memory address and length into 64-bit ABI integer.
    static packDelta(ptr: usize, len: usize): i64 {
        return ((ptr as i64) << 32) | (len as i64);
    }
}

// --- SPATIAL AOI: removal (entity left the AOI set) ------------------------

@external("env", "sys_spatial_remove")
declare function _sys_spatial_remove(entity_id: i64, x: f64, y: f64): i32;

/** Remove a departed entity from the AOI index (pass its last known position). */
export function spatialRemove(entityId: i64, x: f64, y: f64): void {
    _sys_spatial_remove(entityId, x, y);
}

// --- DETERMINISTIC TICK TIMERS (polling model) ------------------------------
// Register an id with a tick delay (and optional repeat interval), then poll
// once per tick (usually first line of onTick). Firing is computed against
// `clockTicks()`, so rollback replays fire identically.

@external("env", "sys_timer_set")
declare function _sys_timer_set(timer_id: i64, delay_ticks: i64, interval_ticks: i64): i32;

@external("env", "sys_timer_cancel")
declare function _sys_timer_cancel(timer_id: i64): i32;

@external("env", "sys_timer_poll")
declare function _sys_timer_poll(): i64;

/** Register timer `id`: fires after `delayTicks`, then every `intervalTicks`
 *  (0 = one-shot). Re-registering an id replaces its schedule. */
export function timerSet(id: i64, delayTicks: i64, intervalTicks: i64 = 0): void {
    _sys_timer_set(id, delayTicks, intervalTicks);
}

/** Stop a timer. */
export function timerCancel(id: i64): void {
    _sys_timer_cancel(id);
}

/** Pop the next fired timer id this tick (0 = none). Call once per tick. */
export function timerPoll(): i64 {
    return _sys_timer_poll();
}

// --- NAVMESH PATHFINDING (project navmesh.nav2d, host-owned) ----------------

@external("env", "sys_navmesh_find_path")
declare function _sys_navmesh_find_path(sx: f64, sy: f64, dx: f64, dy: f64, out_ptr: usize, max_points: i32): i32;

/**
 * A* path across the project navmesh. Fills `outBuf` with f32 x,y pairs
 * (little-endian); returns the point count (0 = no path / no navmesh).
 */
export function navmeshFindPath(sx: f64, sy: f64, dx: f64, dy: f64, outBuf: usize, maxPoints: i32): i32 {
    return _sys_navmesh_find_path(sx, sy, dx, dy, outBuf, maxPoints);
}

// --- RAPIER 3D (full rigid-body world, physics permission) ------------------

@external("env", "sys_rapier3d_init")
declare function _sys_rapier3d_init(gx: f64, gy: f64, gz: f64): i32;
@external("env", "sys_rapier3d_create_body")
declare function _sys_rapier3d_create_body(body_type: i32, x: f64, y: f64, z: f64): i32;
@external("env", "sys_rapier3d_add_ball_collider")
declare function _sys_rapier3d_add_ball_collider(body_id: i32, radius: f64): i32;
@external("env", "sys_rapier3d_add_box_collider")
declare function _sys_rapier3d_add_box_collider(body_id: i32, hx: f64, hy: f64, hz: f64): i32;
@external("env", "sys_rapier3d_set_position")
declare function _sys_rapier3d_set_position(body_id: i32, x: f64, y: f64, z: f64): i32;
@external("env", "sys_rapier3d_set_velocity")
declare function _sys_rapier3d_set_velocity(body_id: i32, vx: f64, vy: f64, vz: f64): i32;
@external("env", "sys_rapier3d_get_position")
declare function _sys_rapier3d_get_position(body_id: i32, out_ptr: i32): i32;
@external("env", "sys_rapier3d_get_velocity")
declare function _sys_rapier3d_get_velocity(body_id: i32, out_ptr: i32): i32;
@external("env", "sys_rapier3d_apply_impulse")
declare function _sys_rapier3d_apply_impulse(body_id: i32, ix: f64, iy: f64, iz: f64): i32;
@external("env", "sys_rapier3d_check_contact")
declare function _sys_rapier3d_check_contact(body_id_a: i32, body_id_b: i32): i32;
@external("env", "sys_rapier3d_remove_body")
declare function _sys_rapier3d_remove_body(body_id: i32): i32;
@external("env", "sys_rapier3d_step")
declare function _sys_rapier3d_step(dt: f64): i32;

/** Reset the room's 3D physics world with the given gravity. */
export function rapier3dInit(gx: f64, gy: f64, gz: f64): void { _sys_rapier3d_init(gx, gy, gz); }
/** Create a body: 0 = dynamic, 1 = fixed, 2/3 = kinematic. Returns its id. */
export function rapier3dCreateBody(bodyType: i32, x: f64, y: f64, z: f64): i32 { return _sys_rapier3d_create_body(bodyType, x, y, z); }
/** Attach a ball collider to a body. */
export function rapier3dAddBallCollider(bodyId: i32, radius: f64): i32 { return _sys_rapier3d_add_ball_collider(bodyId, radius); }
/** Attach a cuboid collider (half-extents) to a body. */
export function rapier3dAddBoxCollider(bodyId: i32, hx: f64, hy: f64, hz: f64): i32 { return _sys_rapier3d_add_box_collider(bodyId, hx, hy, hz); }
export function rapier3dSetPosition(bodyId: i32, x: f64, y: f64, z: f64): void { _sys_rapier3d_set_position(bodyId, x, y, z); }
export function rapier3dSetVelocity(bodyId: i32, vx: f64, vy: f64, vz: f64): void { _sys_rapier3d_set_velocity(bodyId, vx, vy, vz); }
/** Write body position as 3×f32 into `outPtr`; returns 1 on success. */
export function rapier3dGetPosition(bodyId: i32, outPtr: usize): i32 { return _sys_rapier3d_get_position(bodyId, outPtr as i32); }
/** Write body velocity as 3×f32 into `outPtr`; returns 1 on success. */
export function rapier3dGetVelocity(bodyId: i32, outPtr: usize): i32 { return _sys_rapier3d_get_velocity(bodyId, outPtr as i32); }
export function rapier3dApplyImpulse(bodyId: i32, ix: f64, iy: f64, iz: f64): void { _sys_rapier3d_apply_impulse(bodyId, ix, iy, iz); }
/** 1 while the two bodies' colliders overlap or share an active contact. */
export function rapier3dCheckContact(bodyIdA: i32, bodyIdB: i32): bool { return _sys_rapier3d_check_contact(bodyIdA, bodyIdB) == 1; }
export function rapier3dRemoveBody(bodyId: i32): void { _sys_rapier3d_remove_body(bodyId); }
/** Step the 3D world (call once per tick after setting velocities). */
export function rapier3dStep(dt: f64): void { _sys_rapier3d_step(dt); }
