/** Manual clock: tests advance time explicitly, never wait on wall time. */
export class FakeClock {
    constructor(tickRate = 30, startMs = 0) {
        if (tickRate <= 0) throw new RangeError('tickRate must be > 0');
        this.tickRate = tickRate;
        this.intervalMs = 1000 / tickRate;
        this.nowMs = startMs;
        this.tick = 0;
    }

    advanceTicks(count = 1) {
        for (let i = 0; i < count; i++) {
            this.nowMs += this.intervalMs;
            this.tick++;
        }
        return this.nowMs;
    }

    advanceMs(ms) {
        this.nowMs += ms;
        return this.nowMs;
    }

    reset(startMs = 0) {
        this.nowMs = startMs;
        this.tick = 0;
    }
}
