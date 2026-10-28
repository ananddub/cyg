// CYG starter room — Colyseus-style reactive state, server-authoritative.
//
// Flow: onJoin/onInputPacket collect inputs → onTick simulates →
// commitState() encodes the dirty-only delta. The browser rebuilds state
// from the descriptor — no client codegen needed.
//
// Edit this file and run `ox-game run .` — the compiler generates the WASM
// ABI bridge for you.

import { Room, InputReader, Key, isHeld } from "../pkg/cyg-server";
import { State, Player } from "../gen/cyg.schema";

export class MyGame extends Room {
    private s: State = new State();
    private inputs: Map<i64, u8> = new Map<i64, u8>();
    private speed: f64 = 120.0;

    onCreate(max: i32): void {
        this.setState(this.s);
    }

    onJoin(id: i64, x: f64, y: f64): void {
        const p = new Player();
        p.x = x;
        p.y = y;
        this.s.players.set(id as u32, p);
        this.broadcastText("joined", id.toString());
    }

    onLeave(id: i64): void {
        this.s.players.delete(id as u32);
        this.inputs.delete(id);
    }

    // Movement flags from the unreliable lane — store now, apply on tick.
    onInputPacket(id: i64, input: InputReader): void {
        if (this.s.players.has(id as u32)) this.inputs.set(id, input.readU8());
    }

    onTick(dt: f64): i64 {
        const ids = this.inputs.keys();

        for (let i = 0; i < ids.length; i++) {
            const p = this.s.players.get(ids[i] as u32);
            if (p === null) continue;

            const flags = this.inputs.get(ids[i]);
            let dx: f64 = 0;
            let dy: f64 = 0;
            if (isHeld(flags, Key.W)) dy -= this.speed * dt;
            if (isHeld(flags, Key.S)) dy += this.speed * dt;
            if (isHeld(flags, Key.A)) dx -= this.speed * dt;
            if (isHeld(flags, Key.D)) dx += this.speed * dt;
            p.x += dx;
            p.y += dy;
        }
        this.inputs.clear();

        return this.commitState();
    }
}
