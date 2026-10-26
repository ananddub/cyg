/**
 * Browser / Node types for `@cyg/client` (`cyg.js`).
 *
 * The SDK is a UMD bundle. With a bundler:
 *
 * ```ts
 * import { Client, SchemaDef } from "@cyg/client";
 * ```
 *
 * With a script tag the same names hang off `window.CYG`.
 */

export type CYGValue = string | Uint8Array | ArrayBuffer | Record<string, unknown> | null;

export interface ClientOptions {
    token?: string;
}

export interface MatchmakeOptions {
    [key: string]: unknown;
}

export interface SeatReservation {
    name: string;
    sessionId: string;
    roomId: string;
    processId: string;
    reconnectionToken: string;
    seatToken?: string;
    gamePort: number;
    controlPort?: number;
    udpPort?: number;
    iceServers?: string;
    expiresAt?: number;
}

export interface RoomInfo {
    name: string;
    roomId: string;
    clients: number;
    maxClients: number;
    spectators: number;
    metadata: unknown;
    [key: string]: unknown;
}

export type SchemaFieldType =
    | "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "f32" | "f64" | "bool" | "i64";

export interface SchemaField {
    name: string;
    type: SchemaFieldType;
    code: number;
    size: number;
    offset: number;
}

/** Mirrors the server's `SchemaDef`; field order must match exactly. */
export class SchemaDef {
    constructor(name: string, version?: number);
    readonly name: string;
    readonly version: number;
    /** FNV-1a hash of the field list, sent on the wire. */
    readonly hash: number;
    readonly fields: readonly SchemaField[];
    u8(name: string): this;
    i8(name: string): this;
    u16(name: string): this;
    i16(name: string): this;
    u32(name: string): this;
    i32(name: string): this;
    f32(name: string): this;
    f64(name: string): this;
    bool(name: string): this;
    i64(name: string): this;
    indexOf(name: string): number;
}

export type SchemaEntity = { id: number } & Record<string, number | boolean | bigint | string>;

/**
 * `room.state` — a Map of id -> entity. The root collection name from the
 * server's schema descriptor is also exposed as a property, so
 * `room.state.players` is the same map (Colyseus-style access).
 */
export class SchemaMap<V extends { id: number } = SchemaEntity> extends Map<number, V> {
    onAdd(callback: (entity: V, id: number) => void, immediate?: boolean): () => void;
    onRemove(callback: (entity: V, id: number) => void): () => void;
    onChange(callback: (id: number, entity: V) => void): () => void;
    toJSON(): Record<number, V>;
    [collection: string]: any;
}

/** Colyseus v5 callback API, obtained from `getStateCallbacks(room)` (alias: `Callbacks.get(room)`). */
export class SchemaCallbacks {
    listen(property: string, callback: (current: unknown, previous: unknown) => void, immediate?: boolean): () => void;
    listen<T extends object>(instance: T, property: string, callback: (current: unknown, previous: unknown) => void, immediate?: boolean): () => void;
    onChange<T extends object>(instance: T, callback: () => void, immediate?: boolean): () => void;
    onChange(property: string, callback: (key: number, value: SchemaEntity) => void): () => void;
    onAdd(property: string, callback: (value: SchemaEntity, key: number) => void, immediate?: boolean): () => void;
    onRemove(property: string, callback: (value: SchemaEntity, key: number) => void): () => void;
    bindTo<T extends object>(from: object, to: T, properties?: string[], immediate?: boolean): () => void;
}

export const Callbacks: {
    get(room: Room | { state: unknown }): SchemaCallbacks;
    /** Alias matching Colyseus v5's `getStateCallbacks(room)`. */
    getStateCallbacks(room: Room | { state: unknown }): SchemaCallbacks;
    /** Legacy `$(room.state).players.onAdd(...)` proxy. */
    getLegacy(room: Room): SchemaMap;
    getRawChanges(): () => void;
};

/** Colyseus v5 naming parity. Equivalent to `Callbacks.get(room)`. */
export function getStateCallbacks(room: Room | { state: unknown }): SchemaCallbacks;

export class Room<TState extends object = Record<string, unknown>> {
    constructor(endpoint: string, roomName: string, options?: MatchmakeOptions & {
        spectator?: boolean;
        sessionId?: string;
        reconnectionToken?: string;
        autoReconnect?: boolean;
        maxReconnectAttempts?: number;
        /** Set `false` to stay on the WebSocket transport (no WebRTC negotiation). */
        webrtc?: boolean;
        client?: Client;
    });

    readonly endpoint: string;
    readonly roomName: string;
    readonly isSpectator: boolean;
    isOpen: boolean;
    /** Seat id from matchmaking (stable across a resume). */
    seatSessionId: string | null;
    /** Authoritative in-game player id, confirmed by the first delta. */
    sessionId: bigint | null;
    /** Number mirror of `sessionId`, convenient for DOM/UI code. */
    playerId: number | null;
    reconnectionToken: string | null;
    ackedSeq: number;
    serverTick: number;

    /** Bytes received on the WebSocket and WebRTC data channels. */
    bytesIn: number;
    /** Bytes sent through `send`/`sendRaw` (internal control frames excluded). */
    bytesOut: number;

    /**
     * Schema state: id -> entity. Filled automatically from the descriptor the
     * server sends on join. The root collection is also available as
     * `room.state.players` (or whatever the schema names it).
     */
    state: SchemaMap & TState;
    /** Descriptor-derived contract, or the manually attached `useSchema` one. */
    readonly schema: SchemaDef | null;
    readonly schemas: SchemaDef[];
    /** Root collection name from the descriptor ("players", "entities", ...). */
    readonly rootCollection: string | null;

    connect(): Promise<this>;
    leave(): void;

    send(type: string, data?: CYGValue): boolean;
    sendJSON(type: string, value: unknown): boolean;
    sendBytes(type: string, bytes: Uint8Array | ArrayBuffer): boolean;
    sendRaw(bytes: Uint8Array | ArrayBuffer): boolean;
    onMessage(type: string, callback: (payload: Uint8Array | string | null) => void): this;

    on(event: string, callback: (...args: any[]) => void): this;
    off(event: string, callback?: (...args: any[]) => void): this;
    emit(event: string, ...args: any[]): void;

    /**
     * Attach a schema definition manually. Optional: when the project ships an
     * `cyg.schema.bin`, the client attaches it during the join handshake.
     */
    useSchema(schema: SchemaDef): this;
    /** Attach `cyg.schema.bin` locally (live rooms receive it on join). */
    useDescriptor(bytes: Uint8Array | ArrayBuffer): this;
    onAdd(callback: (entity: SchemaEntity, id: number) => void): this;
    onRemove(callback: (entity: SchemaEntity, id: number) => void): this;
    onChange(callback: (entity: SchemaEntity, field: string, value: number | boolean | bigint) => void): this;
    onStateChange(callback: (state: SchemaMap) => void): this;

    /** WebRTC voice mesh, attached on demand. */
    voice: Voice | null;

    /** Interpolation / prediction controls (legacy delta rooms). */
    enableInterpolation(options?: Record<string, unknown>, decoder?: ((bytes: Uint8Array) => unknown) | null): void;
    enableJitterBuffer?(): void;
    disableJitterBuffer?(): void;
}

export interface SchemaSnapshot {
    schema: { name: string; version: number; hash: string; fields: SchemaField[] } | null;
    sessionId: number | null;
    playerId: number | null;
    ready: boolean;
    error: string | null;
    entities: SchemaEntity[];
}

/** Floating schema-state inspector for development. */
export class Inspector {
    constructor(room: Room, options?: { intervalMs?: number });
    snapshot(): SchemaSnapshot;
    attach(target?: HTMLElement | null): this;
    detach(): this;
    render(): void;
}

export class Client {
    constructor(endpoint: string, options?: ClientOptions);
    readonly endpoint: string;
    lastSeat: SeatReservation | null;

    /** Return a token string (or `{ token }`) before each matchmake call. */
    onAuth(callback: () => string | { token: string } | null | Promise<string | { token: string } | null>): this;

    joinOrCreate<TState extends object = Record<string, unknown>>(roomName: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    join<TState extends object = Record<string, unknown>>(roomName: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    create<TState extends object = Record<string, unknown>>(roomName: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    joinById<TState extends object = Record<string, unknown>>(roomId: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    spectate<TState extends object = Record<string, unknown>>(roomName: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    joinSpectator<TState extends object = Record<string, unknown>>(roomName: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    reconnect<TState extends object = Record<string, unknown>>(reconnectionToken: string, roomName: string, options?: MatchmakeOptions): Promise<Room<TState>>;
    getAvailableRooms(roomName?: string | null): Promise<RoomInfo[]>;

    static loadReplay(jsonOrBlob: string | object): ReplayPlayer;
}

export class Voice {
    constructor(...args: any[]);
    [key: string]: any;
}

export class ReplayPlayer {
    constructor(data: unknown);
    play(onDelta?: (bytes: Uint8Array, index: number, total: number) => void): void;
    [key: string]: any;
}

export class DeltaSequencer {
    constructor(...args: any[]);
    [key: string]: any;
}

export class RollbackPredictor {
    constructor(...args: any[]);
    [key: string]: any;
}

export const KeyCode: Record<string, number>;
export const AppOp: Record<string, number>;
export const MeshOp: Record<string, number>;
export const MeshEventKind: Record<string, number>;
export const SignalKind: Record<string, number>;

export function encodeInputFrame(...args: any[]): Uint8Array;
export function encodeStateFrame(...args: any[]): Uint8Array;
export function encodeEventFrame(...args: any[]): Uint8Array;
export function decodeMeshFrame(bytes: Uint8Array): unknown;
export function parseDeltaFrame(bytes: Uint8Array): unknown;
export function encodeDeltaFrame(...args: any[]): Uint8Array;
export function encodeAck(...args: any[]): Uint8Array;
export function isNewerSeq(a: number, b: number): boolean;
