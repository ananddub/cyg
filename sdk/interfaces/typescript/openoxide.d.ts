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
