# @cyg/testing

Deterministic headless room harness for CYG game logic tests.

```js
import { TestRoom } from '@cyg/testing';

const room = await TestRoom.start('arena', {
    tickRate: 30,
    latencyMs: 120,
    loss: 0.05,
    seed: 42,
    step(state, client, input, dt) {
        if (input) client.state.x += input.moveX * dt;
    },
});

const a = await room.connect();
a.sendInput({ moveX: 1 });
room.tick(10);
```

No browser, no server, no wall-clock waits. Latency, loss and RNG are seeded,
so a failing test is reproducible from its seed.
