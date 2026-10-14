// =============================================================================
// OpenOxide Game Server — Room & Client TypeScript Definitions
// =============================================================================

/**
 * Authoritative Game Room Lifecycle Contract
 */
export interface IOpenOxideRoom {
    /** Max concurrent players configured for this room */
    maxClients: number;

    /** Called once when the room is created */
    onCreate(maxClients: number): void;

    /** Called when a player joins with spawn coordinates */
    onJoin(id: bigint, spawnX: number, spawnZ: number): void;

    /** Called when a player leaves or disconnects */
    onLeave(id: bigint): void;

    /** Called when a client sends an authoritative input payload */
    onInput(id: bigint, data: Uint8Array): void;

    /**
     * Fixed tick loop (20 Hz - 128 Hz).
     * Returns the serialized binary state delta buffer or packed pointer.
     */
    onTick(dt: number): Uint8Array | bigint;

    /** State persistence snapshot hook */
    onSave?(): Uint8Array | bigint;

    /** State restoration hook */
    onLoad?(data: Uint8Array): void;

    /** Guest allocation hook */
    onAlloc?(len: number): number;

    /** Capacity check hook */
    onIsFull?(count: number): boolean;
}

/**
 * WebAssembly C-ABI Export Symbols expected by the OpenOxide Rust Engine
 */
export interface IOpenOxideWasmExports {
    on_create(maxClients: number): void;
    on_join(id: bigint, spawnX: number, spawnZ: number): void;
    on_leave(id: bigint): void;
    on_input(id: bigint, ptr: number, len: number): void;
    on_tick(dt: number): bigint;
    on_save(): bigint;
    on_load(ptr: number, len: number): void;
    on_alloc(len: number): number;
    on_is_full(count: number): boolean;
    memory: WebAssembly.Memory;
}

/**
 * OpenOxide Configuration Schema (config.yml)
 */
export interface OpenOxideConfig {
    name: string;
    runtime: 'assemblyscript' | 'rust' | 'cpp' | 'csharp';
    source: string;
    out: string;
    website?: string;
    port: number;
    web_port: number;
    tick_rate: number;
    max_clients: number;
    width: number;
    height: number;
    speed: number;
    abi_version?: number;
    allow_physics?: boolean;
    allow_lag_compensation?: boolean;
    allow_logging?: boolean;
    allow_storage?: boolean;
}

// =============================================================================
// High-Level Developer SDK (AssemblyScript / TypeScript DX Suite)
// =============================================================================

export declare class Vec3 {
    x: number;
    y: number;
    z: number;
    constructor(x?: number, y?: number, z?: number);
    set(x: number, y: number, z: number): this;
    add(other: Vec3): Vec3;
    sub(other: Vec3): Vec3;
    scale(s: number): Vec3;
    length(): number;
    lengthSq(): number;
    distanceTo(other: Vec3): number;
    normalize(): Vec3;
}

export declare class MoveResult {
    x: number;
    y: number;
    z: number;
    isGrounded: boolean;
}

export declare class RaycastHit {
    hasHit: boolean;
    x: number;
    y: number;
    z: number;
    distance: number;
    colliderId: number;
}

export declare class Physics {
    /** Kinematic Move & Slide against host 3D obstacles and ramps */
    static move(px: number, py: number, pz: number, vx: number, vy: number, vz: number, radius: number, dt: number): MoveResult;
    /** Continuous Collision Detection (CCD) 3D Raycast */
    static raycast(ox: number, oy: number, oz: number, dx: number, dy: number, dz: number, maxDist: number): RaycastHit;
    /** Exact ground/ramp height beneath an entity */
    static getSurfaceHeight(px: number, py: number, pz: number, radius?: number): number;
    /** 3D Euclidean distance */
    static distance(x1: number, y1: number, z1: number, x2: number, y2: number, z2: number): number;
    /** 2D Planar distance (XZ) */
    static distance2D(x1: number, z1: number, x2: number, z2: number): number;
    /** 2D Circle-Circle overlap check */
    static checkCircleOverlap(x1: number, z1: number, r1: number, x2: number, z2: number, r2: number): boolean;
    /** 3D Point-in-Sphere check */
    static isPointInSphere(px: number, py: number, pz: number, cx: number, cy: number, cz: number, radius: number): boolean;
    /** 3D Point-in-AABB Box check */
    static isPointInBox(px: number, py: number, pz: number, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): boolean;
}

export declare class Log {
    static info(message: string): void;
    static warn(message: string): void;
    static error(message: string): void;
}

export declare class DeltaWriter {
    constructor(initialCapacity?: number);
    reset(): this;
    ensureCapacity(neededBytes: number): void;
    writeU8(val: number): this;
    writeU16(val: number): this;
    writeU32(val: number): this;
    writeI64(val: bigint): this;
    writeF32(val: number): this;
    writeF64(val: number): this;
    seal(): bigint;
}

export declare class InputReader {
    constructor(ptr: number, len: number);
    get remaining(): number;
    readU8(): number;
    readU32(): number;
    readF32(): number;
    readF64(): number;
    readI64(): bigint;
}

