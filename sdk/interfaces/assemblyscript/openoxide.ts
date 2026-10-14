// =============================================================================
// OpenOxide Game Server — High-Level Guest Room SDK for AssemblyScript
// WebAssembly Target: wasm32 (asc -Ospeed --runtime stub)
// Zero Low-Level Plumbing: No raw pointers, byte offsets, or endianness math!
// =============================================================================

// =============================================================================
// --- 1. INTERNAL HOST C-ABI DECLARATIONS (HIDDEN FROM DEVELOPER) ---
// =============================================================================

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
be 
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
    private _view: DataView;
    private _offset: i32 = 0;

    constructor(initialCapacity: i32 = 1024) {
        let buf = new ArrayBuffer(initialCapacity);
        this._buf = buf;
        this._view = new DataView(buf);
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

/**
 * Authoritative Game Room Base Class for AssemblyScript
 * Extend this class and implement the abstract lifecycle hooks.
 * The OpenOxide compiler automatically creates the Flat WASM ABI bridge!
 */
export abstract class Room {
    maxPlayers: i32 = 16;

    /// Called once when the room instance is allocated on the host.
    abstract onCreate(max: i32): void;

    /// Called when a new player joins the room.
    abstract onJoin(id: i64, spawnX: f64, spawnZ: f64): void;

    /// Called when a player leaves or disconnects.
    abstract onLeave(id: i64): void;

    /// Called when an authoritative client input packet arrives.
    /// By default, wraps the pointer in an InputReader and dispatches to onInputPacket.
    onInput(id: i64, ptr: usize, len: i32): void {
        this.onInputPacket(id, new InputReader(ptr, len));
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

    /// Pack linear memory address and length into 64-bit ABI integer.
    static packDelta(ptr: usize, len: usize): i64 {
        return ((ptr as i64) << 32) | (len as i64);
    }
}
