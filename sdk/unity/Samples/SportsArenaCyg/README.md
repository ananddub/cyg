# Sports Arena Brawler — CYG-native port

This is a **port**, not a drop-in reference replacement. The original sample's
art, prefabs, animation controller and scene layout are reused, but every
simulation and view script runs on CookYourGame.Sim. No reference/reference
assembly, asset database, generated reference code, or reference API is used.

Runnable Unity project used for verification:

```
/mnt/SSD/Devloper/unity/sports_arena_cyg
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
- Optional two-window mode (`-network`) joins an OpenOxide input-hub room. Each
  window publishes its local keyboard input as opcode-19 input ticks and applies
  the peer's latest input to the other human slot. The simulated rules still run
  locally in both clients.
- All six original player spawners and both goal areas are used.
- Original player animator is driven through `Normalized Speed`,
  `Is Grounded`, `Jump`, `Dash`, `Attack`, `Block`, `Has Ball`,
  `Catch Ball`, `Throw Ball Short`, `Is Stunned` and related parameters.
- Minimal IMGUI scoreboard, timer, health bars and control hints.
- PlayMode regression tests: 10/10 passing, including deterministic replay,
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
`YOU ARE P1` / `YOU ARE P2` and the relay status.

## Two-client input relay over OpenOxide

The included `server/config.toml` starts the backend as a pure Unity relay room
(`input_hub = true`, room `cyg_sports`, WebSocket/HTTP port 7777, UDP 7778):

```bash
cargo build --release
./target/release/openoxide-gaming run sdk/unity/Samples/SportsArenaCyg/server
```

Start two players:

```bash
cd /mnt/SSD/Devloper/unity/sports_arena_cyg/Build/Linux
./SportsArena.x86_64 -network -screen-fullscreen 0 -screen-width 960 -screen-height 600
./SportsArena.x86_64 -network -screen-fullscreen 0 -screen-width 960 -screen-height 600
```

`-network` is opt-in; without it the sample remains the offline P1-vs-bots
game. `CYG_NETWORK=1`, `CYG_SERVER_HOST`, `CYG_SERVER_PORT` and `CYG_ROOM` are
also accepted. Both windows call `CYGNetwork.ConnectToMatch` directly and force
`CYGWebSocketTransport`, so the actor id is derived from the same session the
server records. No matchmaker, lobby or reconnect flow is involved.

Each fixed tick sends the local `CYGPlayerInput` as an opcode-19 packet with two
redundant previous inputs. The backend validates room and actor, rate-limits,
and forwards the packet to the other WebSocket clients. The receiving window
applies the latest remote input to its non-local human slot. Rule simulation,
scoring, ball ownership and bots still run independently in each client.

This is input relay, not server-authoritative sports simulation. There is no
shared start tick, rollback or snapshot reconciliation, so latency or a long
stall can make the two local simulations diverge. There is no late join,
reconnect, matchmaking, lobby or character selection.

## What is not the original

This is not 100% feature parity with the reference sample:

- No reference ECS, `Frame`, filters, unsafe component pointers, reference
  assets, signals, `referenceGame`, `referenceRunner`, `referenceEntityView` or
  reference Realtime networking.
- No original menu, lobby, matchmaking, late join, reconnect, character
  selection or ability UI. The optional two-client input relay is not a
  recreation of the original online session flow.
- Original `.qprototype`, `.qunitydb` and baked reference map collision data are
  not read. The port approximates level collision from visible block props.
- Offline, P2-P6 are local deterministic bots. In network mode P2 receives the
  peer's human input, but P3-P6 are still independent local bots and there is
  no server-authoritative match simulation.
- Throw is a single balanced throw; the original short/long throw assets are
  not separately reproduced.
- The port uses a simplified bot, not the original's human-only flow.
- It is not an API-compatibility layer. Existing reference sample scripts still
  cannot compile against this package unchanged.

## Recreate the Unity host project

Requirements: Unity **6000.6.2f1**, URP 17, Input System, and a project that
already references the CookYourGame Unity package
(`com.cookyourgame.sdk`).

```bash
./setup.sh \
  /home/das/Downloads/quantum-sports-arena-brawler-3.0.9 \
  /mnt/SSD/Devloper/unity/sports_arena_cyg
```

The script only reads the original sample. It copies art/prefabs into the
host project, refreshes the CYG scripts and rebuilds the small host scene.

Then open the host project in Unity 6 and play
`Assets/SportsArenaCyg/Scenes/SportsArena.unity`.

## Verification

PlayMode tests were run with:

```bash
Unity -batchmode -nographics \
  -projectPath /mnt/SSD/Devloper/unity/sports_arena_cyg \
  -runTests -testPlatform PlayMode \
  -testResults /tmp/sports_arena_tests.xml
```

The suite includes the SDK's five existing PlayMode physics tests plus five
Sports Arena tests. All ten pass.

A Linux standalone player was also built and launched on the actual display.
Two `-network` instances and one backend were run together. Both clients joined
the same `cyg_sports` relay room, assigned opposite P1/P2 roles, and showed
`PEER ONLINE`. Holding `W` in the P1 window produced `MoveY = 1000` on the P2
window, holding `W` in P2 produced a remote `MoveY = 1000` on P1, and Space
produced the remote Jump button bit. The two windows showed `YOU ARE P1` and
`YOU ARE P2` with cameras following their respective local fighters.

The existing PlayMode suite still passes 10/10 after the network changes.

## Licensing

The CYG runtime code in this folder is covered by the repository's CYG
license. The copied Sports Arena art, models, animation and audio remain
subject to the original sample's license and are **not** redistributed in this
repository; `setup.sh` copies them from the user's own sample installation.
