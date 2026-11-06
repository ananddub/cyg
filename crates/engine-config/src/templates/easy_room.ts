// CYG starter room — reactive state, server-authoritative.
//
// Flow: the client's join options (name, skin, anything) arrive on onJoin →
// onInputFrame collects decoded input → onTick simulates → commitState()
// encodes the dirty-only delta. The browser rebuilds state from the
// descriptor — no client codegen needed.
//
// Edit this file and run `cyg-game run .` — the compiler generates the WASM
// ABI bridge for you.

import {
    Room, Client, SchemaState, Vec2, CloseCode,
} from "../pkg/cyg-server";
import {
    Player, PlayerSchema, Input, JoinOptions,
} from "../gen/cyg.schema";

export class MyGame extends Room {
    private state: SchemaState = new SchemaState(PlayerSchema);
    private players: Array<Player> = [];
    private moves: Map<i64, Vec2> = new Map<i64, Vec2>();
    private speed: f64 = 120.0;

    // options = config room_options ⊕ the creator's joinOrCreate options.
    onCreate(max: i32, options: string): void {
        this.setState(this.state);
    }

    // Typed join options — the schema's JoinOptions def, decoded by the
    // bridge. The client sent { name, team } via joinOrCreate; the room spawns.
    onJoin(client: Client, options: JoinOptions, auth: string): void {
        client.auth = auth;              // what onAuth returned, if you use it
        const id = client.id;
        const p = new Player();
        p.pid = id;
        p.name = options.name == "" ? "player " + id.toString() : options.name;
        p.x = 120.0 + (this.players.length % 8) * 70.0;   // our spawn choice
        p.y = 100.0 + ((this.players.length / 8) as i32) * 70.0;
        this.players.push(p);
        this.moves.set(id, new Vec2(0.0, 0.0));
        this.state.set(id as u32, p);
        client.sendText("welcome", p.name);            // targeted — via the handle
        this.broadcastText("joined", p.name, id); // everyone except them
    }

    // code is a CloseCode (LEFT / KICKED / SERVER_SHUTDOWN / TIMED_OUT / LOST).
    onLeave(client: Client, code: i32): void {
        const id = client.id;
        for (let i = this.players.length - 1; i >= 0; i--) {
            if (this.players[i].pid == id) {
                this.players.splice(i, 1);
                break;
            }
        }
        this.moves.delete(id);
        this.state.delete(id as u32);
    }

    // Typed input lane — the bridge already decoded the frame into your
    // `Input` class. No reader, no decode call. Newest frame wins (stale
    // seqs are dropped for you).
    onInputFrame(client: Client, f: Input): void {
        const id = client.id;
        if (!this.moves.has(id)) return;
        this.moves.set(id, new Vec2(f.moveX, f.moveZ));
    }

    onTick(dt: f64): i64 {
        for (let i = 0; i < this.players.length; i++) {
            const p = this.players[i];
            const move = this.moves.get(p.pid);
            p.x += move.x * this.speed * dt;
            p.y += move.y * this.speed * dt;
        }
        return this.commitState();
    }
}
