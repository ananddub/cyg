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
// --- 2. VECTOR 3D MATH HELPER ---
// =============================================================================

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

    normalize(): Vec3 {
        let len = this.length();
        if (len > 0.00001) {
            return new Vec3(this.x / len, this.y / len, this.z / len);
        }
        return new Vec3(0.0, 0.0, 0.0);
    }
}

// =============================================================================
// --- 3. HIGH-LEVEL ZERO-GC PHYSICS & COLLISION SYSTEM ---
// =============================================================================

export class MoveResult {
    x: f64 = 0;
    y: f64 = 0;
    z: f64 = 0;
    isGrounded: bool = false;
}

export class RaycastHit {
    hasHit: bool = false;
    x: f64 = 0;
    y: f64 = 0;
    z: f64 = 0;
    distance: f64 = 0;
    colliderId: u32 = 0;
}

export class LagRaycastHit {
    hasHit: bool = false;
    hitType: i32 = 0; // 0 = None, 1 = Static Wall, 2 = Entity Hitbox
    x: f64 = 0;
    y: f64 = 0;
    z: f64 = 0;
    distance: f64 = 0;
    entityId: u32 = 0;
    kind: u32 = 0;
    colliderId: u32 = 0;
}

/**
 * Universal High-Level Physics Engine for OpenOxide Game Rooms.
 * Handles continuous collision detection, circles, spheres, ramps, and obstacles seamlessly.
 */
export class Physics {
    private static _buf: ArrayBuffer = new ArrayBuffer(64);
    private static _ptr: usize = changetype<usize>(Physics._buf);
    private static _view: DataView = new DataView(Physics._buf);

    private static _moveRes: MoveResult = new MoveResult();
    private static _rayHit: RaycastHit = new RaycastHit();
    private static _lagRayHit: LagRaycastHit = new LagRaycastHit();

    /**
     * Kinematic Move & Slide:
     * Moves a cylinder/capsule entity (player/bot) along walls, ramps, and Blender obstacles.
     * @returns Pre-allocated MoveResult containing resolved (x, y, z) and isGrounded status.
     */
    static move(px: f64, py: f64, pz: f64, vx: f64, vy: f64, vz: f64, radius: f64, dt: f64): MoveResult {
        let grounded = _sys_physics_move(px, py, pz, vx, vy, vz, radius, dt, Physics._ptr);
        let res = Physics._moveRes;
        res.x = Physics._view.getFloat64(0, true);
        res.y = Physics._view.getFloat64(8, true);
        res.z = Physics._view.getFloat64(16, true);
        res.isGrounded = (grounded == 1);
        return res;
    }

    /**
     * Continuous Collision Detection (CCD) 3D Raycast:
     * Casts a ray through the 3D world against walls, bunkers, pillars, and Blender geometry.
     * @returns Pre-allocated RaycastHit with exact impact coordinates and distance.
     */
    static raycast(ox: f64, oy: f64, oz: f64, dx: f64, dy: f64, dz: f64, maxDist: f64): RaycastHit {
        let hit = _sys_physics_raycast(ox, oy, oz, dx, dy, dz, maxDist, Physics._ptr);
        let res = Physics._rayHit;
        if (hit != 0) {
            res.hasHit = true;
            res.x = Physics._view.getFloat64(0, true);
            res.y = Physics._view.getFloat64(8, true);
            res.z = Physics._view.getFloat64(16, true);
            res.distance = Physics._view.getFloat64(24, true);
            res.colliderId = Physics._view.getUint32(32, true);
        } else {
            res.hasHit = false;
            res.distance = maxDist;
            res.colliderId = 0;
        }
        return res;
    }

    /**
     * Floor, Ramp & Stair Surface Height:
     * Returns the exact solid ground Y coordinate beneath an entity.
     */
    static getSurfaceHeight(px: f64, py: f64, pz: f64, radius: f64 = 0.0): f64 {
        return _sys_physics_surface_y(px, py, pz, radius);
    }

    // --- SHAPE & TRIGGER UTILITIES (CIRCLES, SPHERES, DISTANCES) ---

    /**
     * 3D Euclidean distance between two positions.
     */
    static distance(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64): f64 {
        let dx = x1 - x2;
        let dy = y1 - y2;
        let dz = z1 - z2;
        return Math.sqrt(dx * dx + dy * dy + dz * dz);
    }

    /**
     * 2D Planar distance (XZ plane for top-down or ground gameplay).
     */
    static distance2D(x1: f64, z1: f64, x2: f64, z2: f64): f64 {
        let dx = x1 - x2;
        let dz = z1 - z2;
        return Math.sqrt(dx * dx + dz * dz);
    }

    /**
     * 2D Circle Overlap: returns true if two circles intersect on the ground.
     */
    static checkCircleOverlap(x1: f64, z1: f64, r1: f64, x2: f64, z2: f64, r2: f64): bool {
        let dx = x1 - x2;
        let dz = z1 - z2;
        let minDist = r1 + r2;
        return (dx * dx + dz * dz) <= (minDist * minDist);
    }

    /**
     * 3D Sphere Overlap: returns true if two spheres intersect in 3D space (blast / hitbox).
     */
    static checkSphereOverlap(x1: f64, y1: f64, z1: f64, r1: f64, x2: f64, y2: f64, z2: f64, r2: f64): bool {
        let dx = x1 - x2;
        let dy = y1 - y2;
        let dz = z1 - z2;
        let minDist = r1 + r2;
        return (dx * dx + dy * dy + dz * dz) <= (minDist * minDist);
    }

    /**
     * Point in Sphere: checks if a point lies within a spherical blast radius.
     */
    static isPointInSphere(px: f64, py: f64, pz: f64, cx: f64, cy: f64, cz: f64, radius: f64): bool {
        let dx = px - cx;
        let dy = py - cy;
        let dz = pz - cz;
        return (dx * dx + dy * dy + dz * dz) <= (radius * radius);
    }

    /**
     * Point in AABB Box: checks if a point lies inside an axis-aligned bounding box.
     */
    static isPointInBox(
        px: f64, py: f64, pz: f64,
        minX: f64, minY: f64, minZ: f64,
        maxX: f64, maxY: f64, maxZ: f64
    ): bool {
        return px >= minX && px <= maxX &&
               py >= minY && py <= maxY &&
               pz >= minZ && pz <= maxZ;
    }

    /**
     * Records an entity's 3D hitbox into host history for rewind lag compensation.
     * Call this in onTick for every active player or bot.
     */
    static recordHitbox(
        tick: i64,
        entityId: i32,
        kind: i32,
        minX: f64, minY: f64, minZ: f64,
        maxX: f64, maxY: f64, maxZ: f64
    ): void {
        _sys_physics_lag_record(tick, entityId, kind, minX, minY, minZ, maxX, maxY, maxZ);
    }

    /**
     * Rewind Lag-Compensated Raycast:
     * Rewinds entity hitboxes to client's shoot_tick and performs a continuous raycast.
     * Guarantees "what you see is what you hit" without client-side trust exploits!
     */
    static lagRaycast(
        shootTick: i64,
        ox: f64, oy: f64, oz: f64,
        dx: f64, dy: f64, dz: f64,
        maxDist: f64
    ): LagRaycastHit {
        let hitType = _sys_physics_lag_raycast(shootTick, ox, oy, oz, dx, dy, dz, maxDist, Physics._ptr);
        let res = Physics._lagRayHit;
        if (hitType != 0) {
            res.hasHit = true;
            res.hitType = hitType;
            res.x = Physics._view.getFloat64(0, true);
            res.y = Physics._view.getFloat64(8, true);
            res.z = Physics._view.getFloat64(16, true);
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
        }
        return res;
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
}

// =============================================================================
// --- 7. ABSTRACT ROOM BASE CLASS ---
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
