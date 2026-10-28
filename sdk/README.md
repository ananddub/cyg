# SDK — do alag taraf, do alag kaam

## sdk/client/ — PLAYER ke paas (browser / Unity / Unreal / Godot / native)
Server se BAAT karne ka tarika: connect (WebRTC DataChannel), input bhejo,
server ke state deltas ko decode karke game render karo.
  - web: openoxide-client.js (build se aata hai, /openoxide.js URL pe serve hota hai)
  - unity/ unreal/ godot/ native/ — har engine ka apna transport + helpers

## sdk/server/ — GAME DEV ke paas (room logic likhne ka SDK)
Tenant apna game logic likhta hai — engine sab expose karti hai:
  - Rapier2D / Rapier3D physics
  - Room lifecycle (onJoin/onLeave/onTick), setState/commitState
  - Timers, deterministic RNG, NavMesh, Spatial AOI, Lag-compensation raycast
  - assemblyscript/ = AS me room logic (WASM me compile hota hai, sandboxed)
  - lua/ rust/ c/ cpp/ csharp/ typescript/ — baaki languages ke room SDKs

RULE: server SDK me jo bhi expose hoga (physics, timers, navmesh) tenant
ko freely milega — freedom ka limitation nahi, ye engine ka feature hai.

## Publishing (separate SDK repos)

Each SDK also lives in its own repository for registry publishing
(Unity UPM via git URL, Godot Asset Library, npm):

| SDK | Repo | Install |
|---|---|---|
| Unity | github.com/ananddub/cyg-unity | UPM: `https://github.com/ananddub/cyg-unity.git` |
| Godot | github.com/ananddub/cyg-godot | copy `addons/cyg/` into your project |
| Unreal | github.com/ananddub/cyg-unreal | plugin folder |
| C++ / Lua | github.com/ananddub/cyg-native | headers/sources |
| Web (cyg.js) | github.com/ananddub/cyg-web | `npm install cyg-client` — the server also serves it at `/cyg.js` |

The copies in `sdk/client/` are the development sources; releases to the
SDK repos happen from here when a version ships.
