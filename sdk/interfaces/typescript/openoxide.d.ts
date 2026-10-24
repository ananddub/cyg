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

export declare class CharacterStepResult {
    onGround: boolean;
    isMoving: boolean;
    bodyRot: number;
}

export declare class SweptHitResult {
    hasHit: boolean;
    hitEntityId: bigint;
    t: number;
    isHeadshot: boolean;
    point: Vec3;
}

export declare class Physics {
    /** Kinematic Move & Slide (Friendly Vec3) */
    static move(pos: Vec3, vel: Vec3, radius: number, dt: number): MoveResult;
    /** Kinematic Move & Slide (Raw Coordinates) */
    static moveRaw(px: number, py: number, pz: number, vx: number, vy: number, vz: number, radius: number, dt: number): MoveResult;
    /** Complete Authoritative 3D Kinematics Step for Humanoid/Player Entities */
    static stepHumanoid(
        pos: Vec3,
        vel: Vec3,
        moveDir: Vec2,
        aimYaw: number,
        bodyRot: number,
        radius: number,
        speed: number,
        jumpForce: number,
        gravity: number,
        wantJump: boolean,
        dt: number
    ): CharacterStepResult;
    /** Continuous Collision Detection (CCD) Swept Segment against Entity Cylinders */
    static sweepSegmentAgainstEntities<T extends Entity>(
        from: Vec3,
        to: Vec3,
        entities: Array<T>,
        ignoreId: bigint,
        hitRadius: number,
        maxWallDist?: number
    ): SweptHitResult;
    /** Resolves 2D planar cylinder crowd separation between entities */
    static separateEntities<T extends Entity>(entities: Array<T>, radius: number, maxVerticalDiff?: number): void;
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
    writeI32(val: number): this;
    writeI64(val: bigint): this;
    writeF32(val: number): this;
    writeF64(val: number): this;
    writeBool(val: boolean): this;
    writeBytes(src: ArrayBuffer): this;
    writeWriter(src: DeltaWriter): this;
    readonly length: number;
    readonly buffer: ArrayBuffer;
    writeVec2(v: Vec2): this;
    writeVec3(v: Vec3): this;
    seal(): bigint;
}

export declare enum FieldType {
    U8 = 1,
    U16 = 2,
    U32 = 3,
    I32 = 4,
    F32 = 5,
    F64 = 6,
    BOOL = 7,
    I64 = 8
}

export declare class SchemaDef {
    constructor(name: string, version?: number);
    readonly name: string;
    readonly version: number;
    readonly hash: number;
    readonly fieldCount: number;
    readonly byteSize: number;
    u8(name: string): this;
    u16(name: string): this;
    u32(name: string): this;
    i32(name: string): this;
    f32(name: string): this;
    f64(name: string): this;
    bool(name: string): this;
    i64(name: string): this;
    fieldName(index: number): string;
    fieldType(index: number): FieldType;
    fieldOffset(index: number): number;
    fieldSize(index: number): number;
    indexOf(name: string): number;
}

export declare class SchemaEntity {
    constructor(schema: SchemaDef);
    readonly schemaHash: number;
    readonly dirtyMask: number;
    id: number;
    clearDirty(): void;
    getU8(index: number): number;
    setU8(index: number, value: number): void;
    getU16(index: number): number;
    setU16(index: number, value: number): void;
    getU32(index: number): number;
    setU32(index: number, value: number): void;
    getI32(index: number): number;
    setI32(index: number, value: number): void;
    getF32(index: number): number;
    setF32(index: number, value: number): void;
    getF64(index: number): number;
    setF64(index: number, value: number): void;
    getBool(index: number): boolean;
    setBool(index: number, value: boolean): void;
    getI64(index: number): bigint;
    setI64(index: number, value: bigint): void;
    writeValue(writer: DeltaWriter, index: number): void;
    writeAll(writer: DeltaWriter): void;
    writeDirty(writer: DeltaWriter): void;
}

export declare class SchemaState {
    constructor(schema: SchemaDef);
    readonly schemaHash: number;
    readonly size: number;
    has(id: number): boolean;
    get(id: number): SchemaEntity;
    set(id: number, entity: SchemaEntity): void;
    delete(id: number): void;
    clear(): void;
    seal(): bigint;
}

export declare enum KeyCode {
    None = 0,
    MouseLeft = 1,
    MouseMiddle = 2,
    MouseRight = 3,
    MouseBack = 4,
    MouseForward = 5,
    Backspace = 8,
    Tab = 9,
    Enter = 13,
    Shift = 16,
    Control = 17,
    Alt = 18,
    Pause = 19,
    CapsLock = 20,
    Escape = 27,
    Space = 32,
    PageUp = 33,
    PageDown = 34,
    End = 35,
    Home = 36,
    Insert = 45,
    Delete = 46,
    PrintScreen = 44,
    ArrowLeft = 37,
    ArrowUp = 38,
    ArrowRight = 39,
    ArrowDown = 40,
    Left = 37,
    Up = 38,
    Right = 39,
    Down = 40,
    Digit0 = 48,
    Digit1 = 49,
    Digit2 = 50,
    Digit3 = 51,
    Digit4 = 52,
    Digit5 = 53,
    Digit6 = 54,
    Digit7 = 55,
    Digit8 = 56,
    Digit9 = 57,
    Num0 = 48,
    Num1 = 49,
    Num2 = 50,
    Num3 = 51,
    Num4 = 52,
    Num5 = 53,
    Num6 = 54,
    Num7 = 55,
    Num8 = 56,
    Num9 = 57,
    KeyA = 65,
    KeyB = 66,
    KeyC = 67,
    KeyD = 68,
    KeyE = 69,
    KeyF = 70,
    KeyG = 71,
    KeyH = 72,
    KeyI = 73,
    KeyJ = 74,
    KeyK = 75,
    KeyL = 76,
    KeyM = 77,
    KeyN = 78,
    KeyO = 79,
    KeyP = 80,
    KeyQ = 81,
    KeyR = 82,
    KeyS = 83,
    KeyT = 84,
    KeyU = 85,
    KeyV = 86,
    KeyW = 87,
    KeyX = 88,
    KeyY = 89,
    KeyZ = 90,
    A = 65,
    B = 66,
    C = 67,
    D = 68,
    E = 69,
    F = 70,
    G = 71,
    H = 72,
    I = 73,
    J = 74,
    K = 75,
    L = 76,
    M = 77,
    N = 78,
    O = 79,
    P = 80,
    Q = 81,
    R = 82,
    S = 83,
    T = 84,
    U = 85,
    V = 86,
    W = 87,
    X = 88,
    Y = 89,
    Z = 90,
    MetaLeft = 91,
    MetaRight = 92,
    ContextMenu = 93,
    Numpad0 = 96,
    Numpad1 = 97,
    Numpad2 = 98,
    Numpad3 = 99,
    Numpad4 = 100,
    Numpad5 = 101,
    Numpad6 = 102,
    Numpad7 = 103,
    Numpad8 = 104,
    Numpad9 = 105,
    NumpadMultiply = 106,
    NumpadAdd = 107,
    NumpadSeparator = 108,
    NumpadSubtract = 109,
    NumpadDecimal = 110,
    NumpadDivide = 111,
    F1 = 112,
    F2 = 113,
    F3 = 114,
    F4 = 115,
    F5 = 116,
    F6 = 117,
    F7 = 118,
    F8 = 119,
    F9 = 120,
    F10 = 121,
    F11 = 122,
    F12 = 123,
    F13 = 124,
    F14 = 125,
    F15 = 126,
    F16 = 127,
    F17 = 128,
    F18 = 129,
    F19 = 130,
    F20 = 131,
    F21 = 132,
    F22 = 133,
    F23 = 134,
    F24 = 135,
    NumLock = 144,
    ScrollLock = 145,
    ShiftLeft = 160,
    ShiftRight = 161,
    ControlLeft = 162,
    ControlRight = 163,
    AltLeft = 164,
    AltRight = 165,
    Semicolon = 186,
    Equal = 187,
    Comma = 188,
    Minus = 189,
    Period = 190,
    Slash = 191,
    Backquote = 192,
    BracketLeft = 219,
    Backslash = 220,
    BracketRight = 221,
    Quote = 222
}

export declare class KeyCodeHelper {
    static isLetter(key: KeyCode): boolean;
    static isDigit(key: KeyCode): boolean;
    static isArrow(key: KeyCode): boolean;
    static isFunctionKey(key: KeyCode): boolean;
    static isModifier(key: KeyCode): boolean;
    static isMouseButton(key: KeyCode): boolean;
}

export declare class InputReader {

    constructor(ptr: number, len: number);
    readonly remaining: number;
    hasRemaining(bytes: number): boolean;
    readU8(): number;
    readU16(): number;
    readU32(): number;
    readI32(): number;
    readI64(): bigint;
    readF32(): number;
    readF64(): number;
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

export type MessageHandlerCallback = (senderId: bigint, reader: InputReader, room: Room) => void;
export type RoomEventCallback = (id: bigint, val0: number, val1: number, room: Room | null) => void;

export declare class EventEmitter {
    constructor(owner?: Room | null);
    on(event: string, callback: RoomEventCallback): void;
    emit(event: string, id?: bigint, val0?: number, val1?: number): void;
    clear(): void;
}

export declare abstract class Room {
    maxPlayers: number;
    events: EventEmitter;
    onMessage(type: string, handler: MessageHandlerCallback): void;
    onOpcodeMessage(opcode: number, handler: MessageHandlerCallback): void;
    broadcast(type: string, payload?: ArrayBuffer | null): boolean;
    sendTo(playerId: bigint, type: string, payload?: ArrayBuffer | null): boolean;
    broadcastText(type: string, text: string): boolean;
    sendTextTo(playerId: bigint, type: string, text: string): boolean;
    on(event: string, callback: RoomEventCallback): void;
    emit(event: string, id?: bigint, val0?: number, val1?: number): void;
    abstract onCreate(max: number): void;
    abstract onJoin(id: bigint, spawnX: number, spawnZ: number): void;
    abstract onLeave(id: bigint): void;
    onInput(id: bigint, ptr: number, len: number): void;
    onInputPacket(id: bigint, input: InputReader): void;
    abstract onTick(dt: number): bigint;
    onSave(): bigint;
    onLoad(ptr: number, len: number): void;
    onLoadSnapshot(reader: InputReader): void;
    onAlloc(len: number): number;
    onIsFull(count: number): boolean;
    static packDelta(ptr: number, len: number): bigint;
}



