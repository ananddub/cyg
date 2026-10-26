# Sports Arena Brawler — CYG-native port

This is a **port**, not a drop-in replacement. The original sample's
art, prefabs, animation controller and scene layout are reused, but every
simulation and view script runs on CookYourGame.Sim. No third-party
simulation assembly, asset database or generated code is used.

Runnable Unity project used for verification:

```
~/unity/sports_arena_cyg
```

Play scene:

```
Assets/SportsArenaCyg/Scenes/SportsArena.unity
```

## What works

- Deterministic fixed-point simulation at 60 Hz using `CYGPhysicsWorld`,
  `FPCharacterController`, `FP`, `FPVector3` and `FPQuaternion`.
- Walk/run movement, jump, dash, punch, block, ball pickup, throw, health,
  stun, knockback, respawn, goal scoring, match timer, score-to-win and
  automatic rematch.
- Ball is a CCD-enabled dynamic sphere with bounce/friction and is carried
  above the player while held.
- Static arena collision is built from visible block props plus a solid floor
  and perimeter walls. Environment art remains untouched.
- Six fighters, three per team. Player one is human; player two is a bot by
  default. P2 can be controlled with the keyboard by disabling
  `playerTwoIsBot` on the `Sports Arena` object. Players three to six are bots.
- Optional two-window mode (`-network`) joins an CYG input-hub room.
  Each window publishes tick-keyed keyboard input as opcode-19 packets and only
  advances the deterministic simulation when both humans' inputs for the exact
  tick are present. A HELLO/READY/START handshake gives both clients the same
  tick-0 epoch, and CRC hashes are exchanged every 60 ticks to detect divergence.
  The simulated rules still run locally in both clients.
- All six original player spawners and both goal areas are used.
- Original player animator is driven through `Normalized Speed`,
  `Is Grounded`, `Jump`, `Dash`, `Attack`, `Block`, `Has Ball`,
  `Catch Ball`, `Throw Ball Short`, `Is Stunned` and related parameters.
- Minimal IMGUI scoreboard, timer, health bars and control hints.
- PlayMode regression tests: 16/16 passing, including deterministic replay,
  state-hash divergence detection, reset determinism, the strict input gate,
  bot movement, ball catch, goal scoring and a live bootstrap test.

## Controls

| Player | Move | Jump | Punch | Dash | Throw | Block |
| --- | --- | --- | --- | --- | --- | --- |
| P1 | WASD | Space | J | K | E | L |
| P2 (human) | Arrows | Right Ctrl | Numpad 1 | Numpad 2 | Numpad 3 | Numpad 0 |

Player two is an AI bot unless `playerTwoIsBot` is turned off. Players three
to six are always bots in this port.

In network mode both windows use the P1 row (WASD, Space, J, K, E, L). The
backend orders the two session actors to decide which window owns P1 and which
owns P2; each camera follows that window's own fighter. The HUD shows
`YOU ARE P1` / `YOU ARE P2`, the lockstep state, the current tick, verified hash
count and the last state hash. A mismatch changes the status to `DESYNC` and
stops the simulation instead of hiding the divergence.

## Two-client lockstep over CYG

The included `server/config.toml` starts the backend as a pure Unity relay room
(`input_hub = true`, room `cyg_sports`, WebSocket/HTTP port 7777, UDP 7778):

```bash
cargo build --release
./target/release/cyg-gaming run sdk/unity/Samples/SportsArenaCyg/server
```

Start two players:

```bash
cd ~/unity/sports_arena_cyg/Build/Linux
./SportsArena.x86_64 -network -screen-fullscreen 0 -screen-width 960 -screen-height 600
./SportsArena.x86_64 -network -screen-fullscreen 0 -screen-width 960 -screen-height 600
```

`-network` is opt-in; without it the sample remains the offline P1-vs-bots
game. `CYG_NETWORK=1`, `CYG_SERVER_HOST`, `CYG_SERVER_PORT`, `CYG_ROOM` and
`CYG_INPUT_DELAY` (0-8 ticks, default 2) are also accepted. Both windows call
`CYGNetwork.ConnectToMatch` directly and force `CYGWebSocketTransport`, so the
actor id is derived from the same session the server records. No matchmaker,
lobby or reconnect flow is involved.

Each fixed tick sends the local `CYGPlayerInput` as an opcode-19 packet with two
redundant previous inputs. The packet header carries the exact tick, and the
payload carries the tick in each `CYGPlayerInput`. The backend validates room
and actor, rate-limits, records the tick, and forwards the packet to the other
WebSocket client. The receiver stores all three inputs by their own tick rather
than applying the newest input to its current state.

Both clients run the same tiny lockstep protocol over the backend's
`CustomRpc` tunnel:

1. `HELLO` discovers the peer's derived actor id. Lower actor id becomes P1.
2. `READY` from the peer lets the lower actor send `START` with an epoch and the
   input delay. Both clients call `SportsArenaSimulation.ResetForMatch()` and
   begin at tick 0.
3. Local input is sampled two ticks ahead (`CYG_INPUT_DELAY`, default 2; range
   0-8). A tick advances only when both human inputs for that exact tick are in
   the ring buffer. Missing input pauses rather than inventing a prediction.
4. Every 60 ticks both clients send a CRC32 of the simulation state. If a
   matching tick's hashes differ, both pause and the HUD shows `DESYNC`.

The P3-P6 bots are still simulated locally in each client. Because the rules are
fixed-point and the bots are deterministic functions of state, they stay aligned
as long as the two human input streams and tick schedule do. Automatic rematch
continues from the same tick stream, so the server's monotonic input-tick check
is preserved.

This is strict lockstep, not rollback or server-authoritative sports
simulation. Latency or a long stall waits for the missing input instead of
predicting, so the game can appear to pause. There is no late join, reconnect,
matchmaking, lobby, snapshot reconciliation or server-side arena rules. The
backend remains a relay and validator; it does not simulate the match.


## What is not the original

This is not 100% feature parity with the original sample:

- No original ECS frame/model, filters, unsafe component pointers, baked
  assets, signals, runner/entity-view types or realtime networking.
- No original menu, lobby, matchmaking, late join, reconnect, character
  selection or ability UI. The optional two-client lockstep prototype is not a
  recreation of the original online session flow.
- Original prototype/database files and baked map collision data are
  not read. The port approximates level collision from visible block props.
- Offline, P2-P6 are local deterministic bots. In network mode P2 receives the
  peer's human input through strict tick gating, but P3-P6 are still
  independently simulated local bots and there is no server-authoritative match
  simulation.
- Throw is a single balanced throw; the original short/long throw assets are
  not separately reproduced.
- The port uses a simplified bot, not the original's human-only flow.
- It is not an API-compatibility layer. Existing sample scripts still
  cannot compile against this package unchanged.

## Recreate the Unity host project

Requirements: Unity **6000.6.2f1**, URP 17, Input System, and a project that
already references the CookYourGame Unity package
(`com.cookyourgame.sdk`).

```bash
./setup.sh \
  ~/Downloads/sports-arena-brawler \
  ~/unity/sports_arena_cyg
```

The script only reads the original sample. It copies art/prefabs into the
host project, refreshes the CYG scripts and rebuilds the small host scene.

Then open the host project in Unity 6 and play
`Assets/SportsArenaCyg/Scenes/SportsArena.unity`.

## Verification

PlayMode tests were run with:

```bash
"$UNITY" -batchmode -nographics \
  -projectPath ~/unity/sports_arena_cyg \
  -runTests -testPlatform PlayMode \
  -logFile /tmp/opencode/sports_arena_lockstep_tests4.log
```

The suite includes the SDK's five existing PlayMode physics tests plus eleven
Sports Arena tests. All 16 pass. The new coverage checks state-hash determinism,
state-hash divergence, tick-0 reset determinism, the strict dual-input gate,
future/stale remote-input rejection, and both hash-verification orderings.

A Linux standalone player was built from the same scripts:

```bash
"$UNITY" -batchmode -quit \
  -projectPath ~/unity/sports_arena_cyg \
  -buildLinux64Player \
  ~/unity/sports_arena_cyg/Build/Linux/SportsArena.x86_64
```

Two `-network` instances and one backend were launched on the actual display.
Both joined `cyg_sports` with opposite roles (`P2`/`P1`), received the same
`START epoch=1`, and reported identical hashes:

```
tick=0     hash=0xADEDB7EF
tick=300   hash=0x885E24EE
tick=600   hash=0x6A327EE6
...
tick=12600 hash=0x8BA4A789   verified=210
```

During that run, kernel-level `W` input into the focused P1 window was reported
by the P2 client as `Remote key input received for P1 ... (move=0,1000)`, and
`D` input into the focused P2 window was reported by P1 as
`Remote key input received for P2 ... (move=1000,0)`. Both clients continued to
produce the same hash at every sampled tick with no `DESYNC` log. The final
rebuilt player was launched again and repeated the same handshake, identical
hashes and both input-relay directions.

## Licensing

The CYG runtime code in this folder is covered by the repository's CYG
license. The copied Sports Arena art, models, animation and audio remain
subject to the original sample's license and are **not** redistributed in this
repository; `setup.sh` copies them from the user's own sample installation.
