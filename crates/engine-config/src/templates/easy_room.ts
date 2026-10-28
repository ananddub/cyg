// CYG easy-mode room.
//
// Three channels, kept separate on purpose:
//   this.on / this.emit        local room events (not networked by themselves)
//   onMessage / broadcast      typed, reliable client <-> server messages
//   SchemaState                continuous authoritative state (automatic sync)
//
// The schema itself lives in schema.ts (Colyseus v5 syntax). `ox-game run`
// generates cyg.schema.ts from it, so server and client can never drift.
//
// Edit this file and run `ox-game run .` — the compiler generates the WASM ABI
// bridge for you.

import { Room, InputReader, SchemaState } from "../pkg/cyg-server";
// Player class = generated file se (value + type dono); PlayerSchema = source
import { Player } from "../gen/cyg.schema";

const WORLD_W: f64 = 800.0;
const WORLD_H: f64 = 500.0;
const SPEED: f64 = 220.0;

export class GameRoom extends Room {
    players: Array<Player> = new Array<Player>();
    // Full snapshot + dirty patches are encoded for you. `seal()` returns the
    // packed pointer/len that `onTick` hands back to the engine.
    state: SchemaState = new SchemaState(PlayerSchema);

    // Optional lifecycle hooks — uncomment what you need:
    //
    // onAuth(clientId: i64, options: string): bool {
    //     // `options` is the JSON the client passed to joinOrCreate/join.
    //     return options.includes("token");
    // }
    // allowReconnection(clientId: i64): i32 { return 5000; } // hold seat 5s
    // onReconnect(clientId: i64): void { /* resumed, state kept */ }
    // onDispose(): void { /* room removed: flush scores, ... */ }
    //
    // Rate controls (0 = every host tick, the default):
    // patchRate = 50;          // 20 Hz state frames
    // simulationInterval = 50; // 20 Hz onTick with a fixed dt

    onCreate(maxClients: i32): void {
        this.maxPlayers = maxClients;

        // Local event: useful for room-only side effects. It does not cross the
        // network unless you explicitly broadcast. The SDK injects `room` as
        // the last callback argument, so this stays closure-free.
        this.on("discard", (id: i64, val0: f64, val1: f64, room: Room | null) => {
            if (room != null) {
                room.broadcastText("notice", "player " + id.toString() + " discarded");
            }
        });

        // Client -> server typed message. The web client calls
        // room.send("input", 4-byte-flags).
        this.onMessage("input", (id: i64, reader: InputReader, room: Room) => {
            (room as GameRoom).setInput(id, reader.readU32());
        });

        // Local events and network messages can be composed.
        this.onMessage("discard", (id: i64, reader: InputReader, room: Room) => {
            room.emit("discard", id, 0.0, 0.0);
        });

        // Broadcast a one-shot message to everyone.
        this.onMessage("hello", (id: i64, reader: InputReader, room: Room) => {
            room.broadcast("hello", null);
        });

        // Targeted reply: only the sender receives "pong".
        this.onMessage("ping", (id: i64, reader: InputReader, room: Room) => {
            room.sendTo(id, "pong", null);
        });
    }

    onJoin(id: i64, spawnX: f64, spawnZ: f64): void {
        let p = new Player();
        p.x = spawnX;
        p.y = spawnZ;
        this.players.push(p);
        this.state.set(id as u32, p);
        this.broadcastText("joined", id.toString());
    }

    onLeave(id: i64): void {
        this.state.delete(id as u32);
        for (let i = 0; i < this.players.length; i++) {
            if (unchecked(this.players[i]).id == (id as u32)) {
                this.players.splice(i, 1);
                break;
            }
        }
    }

    onTick(dt: f64): i64 {
        for (let i = 0; i < this.players.length; i++) {
            let p = unchecked(this.players[i]);
            if (p.flags & 1) p.y -= SPEED * dt; // W / Up
            if (p.flags & 2) p.y += SPEED * dt; // S / Down
            if (p.flags & 4) p.x -= SPEED * dt; // A / Left
            if (p.flags & 8) p.x += SPEED * dt; // D / Right

            if (p.x < 14.0) p.x = 14.0;
            if (p.x > WORLD_W - 14.0) p.x = WORLD_W - 14.0;
            if (p.y < 14.0) p.y = 14.0;
            if (p.y > WORLD_H - 14.0) p.y = WORLD_H - 14.0;
        }

        // The first viewer gets the full row; everyone else gets only the
        // fields that changed this tick.
        return this.state.seal();
    }

    setInput(id: i64, flags: u32): void {
        for (let i = 0; i < this.players.length; i++) {
            let p = unchecked(this.players[i]);
            if (p.id == (id as u32)) {
                p.flags = flags;
                return;
            }
        }
    }
}
