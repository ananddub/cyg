// CYG starter room — reactive state, server-authoritative.
//
// Flow: onJoin/onInputPacket collect inputs → onTick simulates →
// commitState() encodes the dirty-only delta. The browser rebuilds state
// from the descriptor — no client codegen needed.
//
// Edit this file and run `ox-game run .` — the compiler generates the WASM
// ABI bridge for you.

import {
    Room, InputReader, SchemaState, Key, isHeld,
} from "../pkg/cyg-server";
import { Player, PlayerSchema } from "../gen/cyg.schema";

export class MyGame extends Room {
    private state: SchemaState = new SchemaState(PlayerSchema);
    private players: Array<Player> = [];
    private speed: f64 = 120.0;

    onCreate(max: i32): void {}

    onJoin(id: i64, x: f64, y: f64): void {
        const p = new Player();
        p.pid = id;
        p.x = x;
        p.y = y;
        this.players.push(p);
        this.state.set(id as u32, p);
        this.broadcastText("joined", p.pid.toString());
    }

    onLeave(id: i64): void {
        for (let i = this.players.length - 1; i >= 0; i--) {
            if (this.players[i].pid == id) {
                this.players.splice(i, 1);
                return;
            }
        }
    }

    // Movement flags from the unreliable lane — stored straight on the
    // player entity (free bonus: clients see facing/keys if they want).
    onInputPacket(id: i64, input: InputReader): void {
        for (let i = 0; i < this.players.length; i++) {
            if (this.players[i].pid == id) {
                this.players[i].flags = input.readU8();
                return;
            }
        }
    }

    onTick(dt: f64): i64 {
        for (let i = 0; i < this.players.length; i++) {
            const p = this.players[i];
            let dx: f64 = 0;
            let dy: f64 = 0;
            if (isHeld(p.flags, Key.W)) dy -= this.speed * dt;
            if (isHeld(p.flags, Key.S)) dy += this.speed * dt;
            if (isHeld(p.flags, Key.A)) dx -= this.speed * dt;
            if (isHeld(p.flags, Key.D)) dx += this.speed * dt;
            p.x += dx;
            p.y += dy;
        }
        return this.commitState();
    }
}
