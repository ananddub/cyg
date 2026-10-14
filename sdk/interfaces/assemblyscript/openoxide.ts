// =============================================================================
// OpenOxide Game Server — Guest Room SDK for AssemblyScript
// WebAssembly Target: wasm32 (asc -Ospeed --runtime stub)
// =============================================================================

// --- 1. HOST C-ABI DECLARATIONS (INTERNAL TO SDK) ---

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

// --- 2. HIGH-LEVEL TYPE-SAFE PHYSICS RESULTS ---

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

/**
 * High-Level, Zero-GC Physics API for OpenOxide Game Rooms.
 * Handles all memory pointers, buffers, and byte-offset decoding behind the scenes.
 */
export class Physics {
    private static _buf: ArrayBuffer = new ArrayBuffer(64);
    private static _ptr: usize = changetype<usize>(Physics._buf);
    private static _view: DataView = new DataView(Physics._buf);

    private static _moveRes: MoveResult = new MoveResult();
    private static _rayHit: RaycastHit = new RaycastHit();

    /**
     * Move an entity with wall sliding and continuous collision detection.
     * @returns Pre-allocated MoveResult containing the resolved (x, y, z) and grounded state.
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
     * Casts a 3D ray against server collision meshes (walls, ramps, obstacles).
     * @returns Pre-allocated RaycastHit with hit details.
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
     * Returns the floor or terrain height directly beneath a world position.
     */
    static getSurfaceHeight(px: f64, py: f64, pz: f64, radius: f64 = 0.0): f64 {
        return _sys_physics_surface_y(px, py, pz, radius);
    }
}

// --- 3. ABSTRACT ROOM BASE CLASS ---

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
    abstract onInput(id: i64, ptr: usize, len: i32): void;

    /// Fixed simulation tick (20 Hz - 128 Hz).
    /// Return packed (ptr << 32) | len for the serialized delta buffer.
    abstract onTick(dt: f64): i64;

    /// State persistence snapshot hook (optional).
    onSave(): i64 {
        return 0;
    }

    /// State restoration hook from saved linear snapshot (optional).
    onLoad(ptr: usize, len: i32): void {
        // Default: no-op
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
