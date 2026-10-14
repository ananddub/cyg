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

export declare class Vec2 {
    x: number;
    y: number;
    constructor(x?: number, y?: number);
    set(x: number, y: number): this;
    copy(other: Vec2): this;
    clone(): Vec2;
    add(other: Vec2): Vec2;
    sub(other: Vec2): Vec2;
    scale(s: number): Vec2;
    length(): number;
    lengthSq(): number;
    distanceTo(other: Vec2): number;
    normalize(): Vec2;
    dot(other: Vec2): number;
    angleTo(other: Vec2): number;
    static fromAngle(rad: number, length?: number): Vec2;
    toString(): string;
}

export declare class Vec3 {
    x: number;
    y: number;
    z: number;
    constructor(x?: number, y?: number, z?: number);
    set(x: number, y: number, z: number): this;
    copy(other: Vec3): this;
    clone(): Vec3;
    add(other: Vec3): Vec3;
    sub(other: Vec3): Vec3;
    scale(s: number): Vec3;
    length(): number;
    lengthSq(): number;
    distanceTo(other: Vec3): number;
    distance2D(other: Vec3): number;
    normalize(): Vec3;
    dot(other: Vec3): number;
    cross(other: Vec3): Vec3;
    yawTo(target: Vec3): number;
    static fromYaw(yawRad: number, speed?: number): Vec3;
    toVec2XZ(): Vec2;
    toString(): string;
}

export declare class MoveResult {
    pos: Vec3;
    x: number;
    y: number;
    z: number;
    isGrounded: boolean;
}

export declare class RaycastHit {
    hasHit: boolean;
    point: Vec3;
    x: number;
    y: number;
    z: number;
    distance: number;
    colliderId: number;
}

export declare class LagRaycastHit {
    hasHit: boolean;
    point: Vec3;
    hitType: number;
    x: number;
    y: number;
    z: number;
    distance: number;
    entityId: number;
    kind: number;
    colliderId: number;
}

export declare class Physics {
    /** Kinematic Move & Slide (Friendly Vec3) */
    static move(pos: Vec3, vel: Vec3, radius: number, dt: number): MoveResult;
    /** Kinematic Move & Slide (Raw Coordinates) */
    static moveRaw(px: number, py: number, pz: number, vx: number, vy: number, vz: number, radius: number, dt: number): MoveResult;
    /** Continuous Collision Detection (CCD) 3D Raycast (Friendly Vec3) */
    static raycast(origin: Vec3, direction: Vec3, maxDist: number): RaycastHit;
    static raycastRaw(ox: number, oy: number, oz: number, dx: number, dy: number, dz: number, maxDist: number): RaycastHit;
    /** Exact ground/ramp height beneath an entity */
    static getSurfaceHeight(pos: Vec3, radius?: number): number;
    static getSurfaceHeightRaw(px: number, py: number, pz: number, radius?: number): number;
    /** 3D Euclidean distance */
    static distance(a: Vec3, b: Vec3): number;
    static distanceRaw(x1: number, y1: number, z1: number, x2: number, y2: number, z2: number): number;
    /** 2D Planar distance */
    static distance2D(a: Vec2, b: Vec2): number;
    static distance2DRaw(x1: number, z1: number, x2: number, z2: number): number;
    /** 2D Circle-Circle overlap check */
    static checkCircleOverlap(posA: Vec2, rA: number, posB: Vec2, rB: number): boolean;
    static checkCircleOverlapRaw(x1: number, z1: number, r1: number, x2: number, z2: number, r2: number): boolean;
    /** 3D Sphere Overlap check */
    static checkSphereOverlap(posA: Vec3, rA: number, posB: Vec3, rB: number): boolean;
    /** 3D Point-in-Sphere check */
    static isPointInSphere(point: Vec3, center: Vec3, radius: number): boolean;
    static isPointInSphereRaw(px: number, py: number, pz: number, cx: number, cy: number, cz: number, radius: number): boolean;
    /** 3D Point-in-AABB Box check */
    static isPointInBox(point: Vec3, min: Vec3, max: Vec3): boolean;
    static isPointInBoxRaw(px: number, py: number, pz: number, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): boolean;
    /** Oriented Bounding Box (OBB) check for rotated obstacles */
    static isPointInRotatedBox(pos: Vec3, center: Vec3, halfWidth: number, halfDepth: number, yawRad: number): boolean;
    static isPointInRotatedBoxRaw(px: number, pz: number, cx: number, cz: number, halfWidth: number, halfDepth: number, yawRad: number): boolean;
    /** Point in Capsule check */
    static isPointInCapsule(point: Vec3, capPos: Vec3, radius: number, height: number): boolean;
    static isPointInCapsuleRaw(px: number, py: number, pz: number, capX: number, capY: number, capZ: number, radius: number, height: number): boolean;
    /** Record entity hitbox for server rewind */
    static recordHitbox(tick: bigint, entityId: number, kind: number, min: Vec3, max: Vec3): void;
    static recordHitboxRaw(tick: bigint, entityId: number, kind: number, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): void;
    /** Rewind lag-compensated continuous raycast */
    static lagRaycast(shootTick: bigint, origin: Vec3, direction: Vec3, maxDist: number): LagRaycastHit;
    static lagRaycastRaw(shootTick: bigint, ox: number, oy: number, oz: number, dx: number, dy: number, dz: number, maxDist: number): LagRaycastHit;
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
    writeVec2(v: Vec2): this;
    writeVec3(v: Vec3): this;
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
    readVec2(): Vec2;
    readVec3(): Vec3;
}

export declare class Random {
    static seed(s: bigint): void;
    static nextU32(): number;
    static nextFloat(): number;
    static range(min: number, max: number): number;
    static rangeFloat(min: number, max: number): number;
    static chance(percentage: number): boolean;
}

export declare class CooldownTracker {
    update(dt: number): void;
    isReady(actionKey: string): boolean;
    trigger(actionKey: string, durationSeconds: number): void;
    getRemaining(actionKey: string): number;
    reset(): void;
}

export declare enum ZoneType {
    NONE = 0,
    DAMAGE = 1,
    SPEED_BUFF = 2,
    SPEED_SLOW = 3,
    JUMP_PAD = 4,
    CAPTURE_ZONE = 5,
    TELEPORT = 6
}

export declare class TriggerZone {
    id: number;
    type: ZoneType;
    minX: number; minY: number; minZ: number;
    maxX: number; maxY: number; maxZ: number;
    value: number;
    constructor(id: number, type: ZoneType, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number, value?: number);
    contains(pos: Vec3): boolean;
    containsRaw(px: number, py: number, pz: number): boolean;
}

export declare enum HitZone {
    BODY = 0,
    HEAD = 1,
    LIMB = 2
}

export declare class DamageResult {
    damageDealt: number;
    shieldAbsorbed: number;
    healthRemaining: number;
    isKilled: boolean;
    isCritical: boolean;
}

export declare class Combat {
    static applyDamage(baseDamage: number, hitZone: HitZone, armor: number, currentShield: number, currentHealth: number): DamageResult;
}

export declare enum MatchPhase {
    WAITING_FOR_PLAYERS = 0,
    COUNTDOWN = 1,
    ROUND_ACTIVE = 2,
    OVERTIME = 3,
    ROUND_OVER = 4,
    RESETTING = 5
}

export declare class MatchStateMachine {
    phase: MatchPhase;
    timer: number;
    roundNumber: number;
    startCountdown(durationSeconds?: number): void;
    startRound(roundDurationSeconds?: number): void;
    endRound(postRoundSeconds?: number): void;
    update(dt: number): boolean;
}

export declare class SpatialGrid2D {
    update(id: bigint, pos: Vec3): void;
    updateRaw(id: bigint, x: number, z: number): void;
    remove(id: bigint): void;
    queryRadius(center: Vec3, radius: number): bigint[];
    queryRadiusRaw(cx: number, cz: number, radius: number): bigint[];
}

export declare class Steering {
    static seek(from: Vec3, target: Vec3, maxSpeed: number): Vec3;
    static seekRaw(fromX: number, fromZ: number, targetX: number, targetZ: number, maxSpeed: number): Vec3;
    static flee(from: Vec3, threat: Vec3, maxSpeed: number): Vec3;
    static fleeRaw(fromX: number, fromZ: number, threatX: number, threatZ: number, maxSpeed: number): Vec3;
    static arrive(from: Vec3, target: Vec3, maxSpeed: number, slowingDistance: number): Vec3;
    static arriveRaw(fromX: number, fromZ: number, targetX: number, targetZ: number, maxSpeed: number, slowingDistance: number): Vec3;
}

export declare class Entity {
    id: bigint;
    pos: Vec3;
    vel: Vec3;
    rot: Vec3;
    radius: number;
    health: number;
    shield: number;
    armor: number;
    isAlive: boolean;
    distanceTo(other: Entity): number;
    lookAt(target: Vec3): void;
}
