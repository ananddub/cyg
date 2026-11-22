import type { Room as EngineRoom, MatchmakeOptions, CYGValue } from "./types/engine";
import engine from "./core/engine.js";

/**
 * Contract between a game and its generated schema bindings
 * (`cyg_schema_generated.d.ts` from `ox-game schema-codegen . --lang ts`).
 */
export interface RoomContract<
    TState extends object,
    TMessages extends Record<string, unknown>,
> {
    /** Typed root state exposed as `room.state` (e.g. `GameState`). */
    state: TState;
    /** Typed message/input payloads, keyed by wire message name. */
    messages: TMessages;
}

type EngineOptions = NonNullable<ConstructorParameters<typeof EngineRoom>[2]>;

/**
 * A room whose messages and state are typed by the generated schema contract.
 * Thin compile-time layer over the engine `Room` — all transport, decode and
 * prediction work stays in the shared `cyg-core-rs` wasm core.
 */
export class TypedRoom<
    TState extends object,
    TMessages extends Record<string, unknown> = Record<string, never>,
> {
    readonly inner: EngineRoom;
    /** Escape hatch to the untyped engine room. */
    readonly raw: EngineRoom;

    constructor(endpoint: string, roomName: string, options?: EngineOptions) {
        const room = new engine.Room(endpoint, roomName, options) as unknown as EngineRoom;
        this.inner = room;
        this.raw = room;
    }

    connect(): Promise<this> {
        return this.inner.connect().then(() => this);
    }

    leave(): void {
        this.inner.leave();
    }

    ping(callback?: (ms: number) => void): Promise<number> {
        return this.inner.ping(callback);
    }

    /** Typed send: `room.send("move", { x, y })` fails to compile for an unknown key or bad payload. */
    send<K extends keyof TMessages & string>(
        type: K,
        data: TMessages[K],
    ): boolean {
        return this.inner.send(type, data as CYGValue);
    }

    sendBytes(type: string, bytes: Uint8Array | ArrayBuffer): boolean {
        return this.inner.sendBytes(type, bytes);
    }

    sendJSON(type: string, value: unknown): boolean {
        return this.inner.sendJSON(type, value);
    }

    /**
     * Typed message receive. The payload is validated by the game schema at
     * runtime; the compile-time type comes from `TMessages[K]`.
     */
    onMessage<K extends keyof TMessages & string>(
        type: K,
        callback: (payload: TMessages[K]) => void,
    ): this {
        this.inner.onMessage(type, (payload) => {
            callback(payload as TMessages[K]);
        });
        return this;
    }

    /** Typed root state notification: `room.onStateChange((state) => state.players)`. */
    onStateChange(callback: (state: TState) => void): this {
        this.inner.onStateChange((state) => callback(state as unknown as TState));
        return this;
    }

    on(event: string, callback: (...args: unknown[]) => void): this {
        this.inner.on(event, callback);
        return this;
    }

    off(event: string, callback?: (...args: unknown[]) => void): this {
        this.inner.off(event, callback);
        return this;
    }

    /** Current typed root state (null before the first snapshot). */
    get state(): TState | null {
        return (this.inner.state as unknown as TState) ?? null;
    }

    get isOpen(): boolean {
        return this.inner.isOpen;
    }

    get seatSessionId(): string | null {
        return this.inner.seatSessionId;
    }

    get playerId(): number | null {
        return this.inner.playerId;
    }

    get reconnectionToken(): string | null {
        return this.inner.reconnectionToken;
    }

    get clock(): EngineRoom["clock"] {
        return this.inner.clock;
    }
}

