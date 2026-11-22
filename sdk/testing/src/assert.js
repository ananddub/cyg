/** Resolve once `predicate()` is true, after advancing the room in steps. */
export async function assertEventually(room, predicate, { ticks = 60, step = 1 } = {}) {
    for (let i = 0; i < ticks; i++) {
        if (predicate()) return;
        room.tick(step);
        await Promise.resolve();
    }
    throw new Error('assertEventually: condition not met within tick budget');
}

/** Plain serialisable snapshot of a room state map. */
export function snapshot(state) {
    const players = {};
    for (const [id, player] of state.players || []) players[id] = { ...player };
    return { tick: state.tick, players };
}
