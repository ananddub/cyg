// PvP Shooting E2E Test: 2 bots connect via @colyseus/sdk.
// Shooter aims at Target, shoots bullets, verifies target HP drops and damage is dealt.
import { Client } from '/mnt/SSD/Devloper/pannel/colyseus/packages/sdk/build/index.mjs';

const client1 = new Client('ws://127.0.0.1:7777');
const client2 = new Client('ws://127.0.0.1:7777');

const timeout = setTimeout(() => {
    console.error('RESULT: TIMEOUT — PvP test failed');
    process.exit(1);
}, 12000);

try {
    const room1 = await client1.joinOrCreate('game_room');
    console.log('[pvp] Shooter joined:', room1.sessionId);

    const room2 = await client2.joinOrCreate('game_room');
    console.log('[pvp] Target joined:', room2.sessionId);

    let shooterId = null;
    let targetId = null;
    let targetHpStart = null;
    let hitConfirmed = false;
    let killConfirmed = false;

    function decodeFrame(bytes) {
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const myId = view.getBigInt64(0, true);
        let offset = 8;
        const count = view.getUint32(offset, true);
        offset += 4;

        const players = [];
        for (let i = 0; i < count; i++) {
            const id = view.getBigInt64(offset, true);
            offset += 8;
            const x = view.getFloat64(offset, true);
            offset += 8;
            const y = view.getFloat64(offset, true);
            offset += 8;
            const score = view.getFloat64(offset, true);
            offset += 8;
            const hp = view.getFloat64(offset, true);
            offset += 8;
            players.push({ id, x, y, score, hp });
        }
        return { myId, players };
    }

    room2.onMessage("delta", (bytes) => {
        const { myId, players } = decodeFrame(bytes);
        targetId = myId;
        const me = players.find(p => p.id === targetId);
        if (me && targetHpStart === null) {
            targetHpStart = me.hp;
            console.log(`[pvp] Target id=${targetId} initial HP=${me.hp}`);
        }
        if (me && targetHpStart !== null && me.hp < targetHpStart) {
            if (!hitConfirmed) {
                hitConfirmed = true;
                console.log(`[pvp] 💥 HIT CONFIRMED! Target HP dropped: ${targetHpStart} -> ${me.hp}`);
            }
        }
    });

    let frameCount = 0;
    room1.onMessage("delta", (bytes) => {
        frameCount++;
        const { myId, players } = decodeFrame(bytes);
        shooterId = myId;

        const shooter = players.find(p => p.id === shooterId);
        const target = players.find(p => p.id !== shooterId);

        if (!shooter || !target) return;

        // Calculate aim angle towards target
        const angle = Math.atan2(target.y - shooter.y, target.x - shooter.x);

        // Send SHOOT input (bit 16) + angle
        const buf = new ArrayBuffer(8);
        const view = new DataView(buf);
        view.setUint32(0, 16, true); // shoot flag
        view.setFloat32(4, angle, true);
        room1.sendBytes('input', new Uint8Array(buf));

        if (frameCount % 30 === 1) {
            console.log(`[pvp] Shooter id=${shooterId} shooting at Target id=${target.id} (target HP=${target.hp.toFixed(0)}, shooter score=${shooter.score})`);
        }

        if (shooter.score >= 50 || (hitConfirmed && frameCount > 40)) {
            clearTimeout(timeout);
            console.log(`[pvp] 🎯 SUCCESS! PvP Shooting Verified! Target HP=${target.hp.toFixed(0)}, Shooter Score=${shooter.score}`);
            console.log('RESULT: SUCCESS');
            process.exit(0);
        }
    });

} catch (err) {
    console.error('[pvp] Error:', err);
    process.exit(1);
}
