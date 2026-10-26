# 🍳 CookYourGame (CYG) Unity SDK

Ultra-lightweight, zero-allocation drag-and-drop multiplayer networking client for Unity.
Designed to replace legacy, expensive netcode solutions with an ultra-affordable, high-performance architecture.

---

## 🚀 Quick Start (In 3 Minutes)

### 1. Installation
In Unity, open **Window > Package Manager**, click **+ > Add package from git URL...** and enter:
```
https://github.com/cookyourgame/cookyourgame-unity.git
```

---

### 2. Connect to Server & Join a Room

Create a simple C# script (e.g. `GameLobby.cs`):

```csharp
using UnityEngine;
using CookYourGame;

public class GameLobby : MonoBehaviour
{
    void Start()
    {
        // 1. Connect to your server
        CYGNetwork.Connect("ws://localhost:4000/ws");

        // 2. Listen for connection event
        CYGNetwork.OnConnectedToServer += () =>
        {
            Debug.Log("Connected! Joining Room...");
            CYGNetwork.JoinOrCreateRoom("MainRoom", maxPlayers: 4);
        };

        // 3. Spawn player when room is joined
        CYGNetwork.OnJoinedRoom += (roomName) =>
        {
            Debug.Log($"Joined {roomName}! Spawning player...");
            CYGNetwork.Instantiate("PlayerPrefab", new Vector3(0, 1, 0), Quaternion.identity);
        };
    }
}
```

---

### 3. Make Any GameObject Multiplayer (Drag & Drop)

1. Open your **Player Prefab** (placed inside a `Resources/` folder).
2. Add component: **`CYGView`**
3. Add component: **`CYGTransformSync`** (Check Position and Rotation)

**That's it! Play the game in two windows — your players are moving in real time with smooth interpolation and lag compensation!**

---

### 4. Who is the Host? (`IsMasterClient`)

Easily run host-only logic (like spawning enemies or timers) without server-side code:

```csharp
if (CYGNetwork.IsMasterClient)
{
    // Only the Master Client runs this:
    SpawnCoins();
}
```

---

### 5. Remote Procedure Calls (`[CYGRPC]`)

Call methods on other machines across the network:

```csharp
using UnityEngine;
using CookYourGame;

public class PlayerCombat : MonoBehaviour
{
    private CYGView view;

    void Awake() => view = GetComponent<CYGView>();

    void Update()
    {
        if (view.IsMine && Input.GetKeyDown(KeyCode.Space))
        {
            // Call FireWeapon on all other players
            view.RPC("FireWeapon", CYGRPCTarget.Others, 100);
        }
    }

    [CYGRPC]
    void FireWeapon(int damage)
    {
        Debug.Log($"Player fired weapon with {damage} damage!");
        // Play muzzle flash / sound effect
    }
}
```

---

## 📦 Package Layout

```
Runtime/
  CookYourGame.Runtime.asmdef
  Core/            CYGNetwork, CYGNetworkManager, CYGView, CYGBehaviour,
                   CYGPlayer, CYGOpCodes, CYGServerSettings, CYGPrefabRegistry,
                   CYGSceneManager
  Transport/       CYGTransport (wire framing + WebSocket), CYGProfilingTransport
  Serialization/   CYGBitPacker, CYGDeltaPacker, CYGSyncVar, CYGRPC
  Components/      CYGTransformSync, CYGAnimatorSync, CYGHierarchySync,
                   CYGOwnershipToggle, CYGServerToggle, CYGVisibility
  Resilience/      CYGSessionCookie, CYGLagCompensation
  Pooling/         CYGObjectPool
  Profiler/        CYGNetworkProfiler
  Sim/             optional fixed-point ECS world, snapshots, rollback and input hub
  Sim/CodeGen/     Roslyn source generator for deterministic components
  Sim/Physics3D/   deterministic rigid bodies, character controller, static meshes
  Sim/UnityAdapter/ runners, views, countdowns, asset database, anim/desync adapters
  BaaS/            CYGAuth, CYGHttp, CYGStorage, CYGInventory, CYGLeaderboard,
                   CYGChat
Samples/CYG/       arena.cyg and types.cyg DSL samples
Samples/SportsArenaCyg/  CYG-native playable port of an arena brawler reference sample
Editor/
  CookYourGame.Editor.asmdef
  CYGProfilerWindow.cs
  CYGMultiClientWindow.cs
```

### What each area gives you

* **`Samples/SportsArenaCyg/`** — a playable, CYG-native port of an arena brawler reference sample. It reuses the original art but none of the original simulation code. This is a port, not a drop-in compatibility layer; see its `README.md` for the supported feature set and setup script.
* **`Core/`** — session and identity. `CYGNetwork` (connect, rooms, master client, spawn/destroy), `CYGView` + `CYGBehaviour` (ownership via `IsMine`, spawn/despawn/owner-changed hooks, the 8-slot delta pump).
* **`Transport/`** — `[len u16 BE][op u8][payload]` framing over a thread-safe WebSocket client. Install `CYGProfilingTransport` through `CYGNetwork.TransportFactory` to instrument bytes without touching the core.
* **`Serialization/`** — `CYGBitWriter`/`CYGBitReader` (varint, half-float, packed vector, smallest-three quaternion), plus `CYGDeltaPacker` for binary diff/patch of state snapshots.
* **`Components/`** — the drag-and-drop `MonoBehaviour`s. `CYGHierarchySync` rides the `StateDelta` channel, so attaching an object to a vehicle needs no new opcodes.
* **`Resilience/`** — `CYGSessionCookie` resumes a dropped seat inside a 30s window with capped backoff; `CYGLagCompensation` rewinds collider history for hit registration.
* **`Pooling/`** — `CYGObjectPool` backs `CYGNetwork.SpawnPrefab`/`DestroyObject`; pooled objects keep their stale `ViewId` until the caller reassigns it.
* **`Profiler/`** — rolling one-second byte/packet counters per opcode, an in-game overlay (Left Ctrl+P) and an editor window. Stripped from shipping builds via `#if UNITY_EDITOR || DEVELOPMENT_BUILD`.
* **`BaaS/`** — account, storage, inventory, leaderboard and chat over the control plane's `/v2/` routes. `CYGChat` reads and writes channel messages (build ids with `RoomChannelId`/`GroupChannelId`/`DirectChannelId`), keeps a bounded local timeline and raises `MessageReceived`/`MessagesUpdated` for the UI.

---

## 🧮 Optional Deterministic Simulation

`Runtime/Sim` is an **opt-in** third mode; it does not replace the default
server-authoritative or relay paths. Enable it only in a project whose
simulation is written against the fixed-point `FP*` types and
`FPCharacterController`, not Unity `Rigidbody`/PhysX.

The input hub (wire opcode `19`) is also opt-in on the server
(`input_hub = true`). It authenticates a peer's actor id from its session,
scopes packets to the joined room, checks framing, tick monotonicity, payload
shape and rate, then forwards validated inputs. That is tamper-resistance for
the *transport*, **not anti-cheat**: every client can still see all inputs and
state, and a modified client can produce any legal input. Competitive/FPS
titles should use server-authoritative simulation with server collision and
line-of-sight checks instead.

### Cross-device determinism verification

The tick math is integer-only Q32.32. `float`/`double` exist only at the
conversion boundary (`FP.FromFloat`, `AsFloat`, rendering adapters); the
fixed-point operators, trig, collision, snapshot and rollback paths do not use
them. A `float`/`double` field in a `[CYGComponent]` is a determinism bug even
though the generator only warns (`CYG010`).

The canonical probe (`CYGDeterminismProbe`) runs 10,000 mixed operations and
reduces the raw values to CRC-32 and FNV-1a 64. The hashes are pinned in code,
so a mismatch fails instead of quietly passing a local run-to-run check:

```bash
dotnet run --project Runtime/Sim/Tests/DeterminismCheckTest.csproj -c Release
# SHA-256  43544fe45821264e85e4f16a11925103d9c811ed7f4b40899ff15384aa5ed9af
# CRC-32   0xE9D6E0FF
# FNV-1a64 0x76E67011297DCBA1
```

In Unity, run `CookYourGame > Verify Physics Determinism`, or add the
`CYG Determinism Self Check` component to a bootstrap scene and run a player
build on every target. It logs, for example:

```
[CYG] DETERMINISM OK platform=Android backend=IL2CPP unity=... crc=0xE9D6E0FF fnv=0x76E67011297DCBA1 ...
```

Every device must log the same `crc` and `fnv`. Enable `Dump Stream` to write
`cyg_determinism_stream.bin` under `persistentDataPath`; a byte diff
(`cmp` or `fc /b`) reports the first diverging operation.

Verified so far: .NET 8 (Linux x64), Unity 6000.6.2f1 Mono (Linux x64),
browser .NET WASM, Android IL2CPP `arm64-v8a` (x86_64 emulator with ARM
translation) and WebGL IL2CPP (headless Chrome). iOS and a physical ARM64
phone still need one run each; the harness makes that a log line instead of an
assumption.

The probe covers the primitives. For a concrete game, also log
`CYGWorld.ComputeChecksum()` at the same tick on each device; the first
mismatching tick points at the system that diverged. Run the probe on each
target and record the result in CI.

The automated rollback path has its own two-client harness: an authoritative
server plus two predicting clients exchange redundant inputs over a simulated
lossy/jittery link, repair gaps in both directions, and compare every per-frame
checksum in a final report.

```bash
dotnet run -c Release --project "Tools~/CYG.Headless.Match" -- --mode all
```

It covers kinematic movement, 3D physics, 2D physics and navigation, and fails
on a desync, a rejected rollback or an unverified frame.

This proves math and build parity, not input parity: every peer must simulate
the same integer `CYGPlayerInput` bytes for the same tick. A float sampled from
a device (analog stick, `Input.GetAxis`) is not cross-device stable by itself;
canonicalize it to integer input before it enters the sim.

The input hub relays inputs; it does not ship state snapshots or a
server-driven tick epoch. Treat all peers as starting together at tick 0
(a loading/start barrier) and do not allow late join or mid-match reconnect
without a game-level resync. The current mode is a lockstep prototype, not a
drop-in replacement for a server-simulated deterministic room.

### Entity view lifecycle

`CYGEntityView` is the presentation root for a deterministic entity. The
simulation owns prefab identity through the `CYGViewBinding` ECS component
(not the network `CookYourGame.CYGView` identity component). `CYGViewBinding`
stores a stable prefab asset id, so a view can be recreated after a snapshot
restore or by a simulation system on every client:

```csharp
CYGViewBinding binding = CYGViewBinding.Create(
    new CYGAssetId(prefabAsset.AssetId),
    CYGViewBinding.BindVerified,
    CYGViewBinding.FlagPool | CYGViewBinding.FlagVisible);

world.Set(entity, binding); // raw CYGWorld: call world.Register<CYGViewBinding>() first
// or: runner.SetEntityView(entity, prefabKey); // runner registers the store for you
```

The lifecycle is `OnInitialize` once, `OnActivate`/`OnDeactivate` per
spawn/pool cycle, `OnUpdateView` during the visual frame, and
`OnLateUpdateView` after transform/camera work. Disabled view components and
inactive views are skipped. `CYGDeterministicRunner` is not auto-created by
view scripts: add one to the scene or call `runner.Initialize()` explicitly.

Input can use the existing `CYGCallbacks` override or the callback poll
event:

```csharp
callbacks.InputPoll += poll =>
{
    if (!poll.IsInputSet)
    {
        poll.SetInput(new CYGPlayerInput(poll.Tick, 0, moveX, moveY));
    }
};
```

### Countdown timers

`CYGCountdownTimer` is a fixed-point timer for dash cooldowns, ability
charges, respawn delays and match phases. It is deterministic and stores only
simulation state, so it rolls back with the world. Register the companion
component and let the built-in system tick it:

```csharp
CYGCountdown dash = CYGCountdown.Create(FP.FromFraction(3, 2)); // 1.5s
world.Set(entity, dash); // caller registers CYGCountdown and adds CYGCountdownSystem
```

`CYGDeterministicRunner` adds `CYGCountdownSystem` automatically
(`enableCountdownTimers`, on by default). Subscribe to
`CYGCountdownSystem.OnExpired` for the expiry edge, or read
`timer.Progress` / `timer.Normalized` for fill bars. `Tick` returns true only
on the expiry tick; looping timers wrap without firing every frame.

### Static mesh colliders for level maps

`CYGStaticMeshAuthoring` bakes Unity `MeshCollider` geometry (ramps, slopes,
walls, tunnels) into world-space fixed-point triangles at startup. Baked meshes
are registered by a stable id and can be queried without UnityEngine:

```csharp
CYGStaticMesh mesh = CYGMeshRegistry.Get(meshId);
mesh.Raycast(ray, maxDistance, out FPRaycastHit hit);
mesh.CapsuleCast(capsule, direction, maxDistance, out hit);
```

For character movement, feed the baked meshes into the deterministic collision
world and use `FPCharacterController`:

```csharp
SimpleCollisionWorld collision = new SimpleCollisionWorld();
runner.PopulateCollisionWorld(collision); // baked level meshes
controller.Move(displacement, collision);
```

The runner exposes `StaticMeshes` and `CollisionWorld`. In `Physics3D` mode the
runner automatically registers every baked mesh as a static triangle body, so
dynamic spheres, capsules and boxes collide with level geometry as well as
with primitive colliders. Box and capsule bodies are fully oriented: their
rotation feeds SAT, mesh contacts, ray/shape queries and torque through a
diagonal local inertia tensor. You can also wire meshes manually:

```csharp
CYGPhysics3DSystem physics = new CYGPhysics3DSystem();
foreach (CYGStaticMesh mesh in runner.StaticMeshes)
{
    physics.AddStaticMesh(mesh);
}
```

Mesh bodies are static-only immutable content and stay outside world snapshots;
dynamic bodies, contacts, queries, character movement and CCD all resolve them
by stable mesh id. Imported meshes must have **Read/Write** enabled in their
model import settings so the baker can read their vertices.

### Asset database

`CYGAssetDatabase` maps a stable integer id (or FNV-1a key) to a Unity asset.
At startup the runner copies it into `CYGAssetRegistry.Shared`, so ECS systems
read balance data without touching Unity objects:

```csharp
// Edit-time: one row per ScriptableObject, id derived from the key.
if (CYGAssetRegistry.Shared.TryGet(weaponId, out WeaponStatsAsset stats))
{
    damage = stats.Damage;
}
```

Lookups work in the Unity player, headless tools and tests. The registry is
static content and is deliberately excluded from snapshots; do not mutate an
asset during a match.

### Simulation-driven animation

`CYGSimAnimationDriver` is a `CYGEntityViewComponent` that reads
`CYGSimAgent` every visual frame and writes `Speed`, `Grounded`,
`VerticalSpeed` and an `Attack` trigger to a Unity `Animator`. The movement
system stores the primary action button in `CYGSimAgent.Attacking`, so the
driver fires the attack trigger once on the rising edge without any per-game
button wiring. `TriggerAttack()` remains available for scripted or custom
attacks. The driver is presentation-only and does not affect determinism. Use
it instead of `CYGAnimatorSync` for local sim presentation; the latter mirrors
Animator parameters over the network for remote peers.

### Desync frame differ overlay

Add `CYGDesyncOverlay` next to the runner (F8 toggles it). While enabled it
keeps a short ring of world snapshots, subscribes to
`CYGVerifiedFrameTracker.OnDesync`, and prints the first divergent
entity/component/RNG state from `CYGDesyncDiagnostics` on screen. Custom
network code can call `SetReferenceSnapshot(tick, snapshot)` to diff against a
server-provided snapshot, or `ReportDesync(tick, local, remote)`. Snapshot
capture allocates, so disable it in shipping builds.

### In-editor multi-client runner

Open `CookYourGame > Sim > Multi-Client Runner`. The window steps two to four
independent `CYGWorld` instances in lockstep with the same bot input stream and
reports each client's checksum, entity count, position and the first divergent
tick. This catches determinism regressions without launching a server. The same
window can launch N standalone player instances from a build path for a real
network test.



Hand-written `WriteTo`/`ReadFrom`/checksum code is easy to get wrong. The
`CookYourGame.Sim` source generator (`Tools~/CYG.CodeGen`) lets you declare only
the state:

```csharp
using CookYourGame.Sim;

[CYGComponent(Id = 1001)]
public partial struct CharacterState
{
    public FP Health;
    public FPVector3 Velocity;
    public bool Grounded;
}
```

The generated partial supplies `SizeInBytes`, `TypeId`, `SchemaHash`, explicit
little-endian `WriteTo`/`ReadFrom`, `ComputeChecksum`, and `CopyTo`. It rejects
reference fields, unsupported nested values, explicit layout, generic
components, and non-partial declarations. See
`Tools~/CYG.CodeGen/README.md` for build and diagnostic details.

### `.cyg` DSL (schema-based authoring)

Instead of writing the C# component skeleton by hand, declare the whole
simulation schema in `.cyg`. The CLI emits plain partial struct declarations;
the same Roslyn generator above emits the wire codecs. There is exactly one
serializer: sizes, `WriteTo`/`ReadFrom`, checksums and `SchemaHash` all come
from one declaration.

```cyg
namespace Game;

component Transform : 1002 {
    FPVector3 Position;
    FPQuaternion Rotation;
    FPVector3 Scale;
}

enum PlayerStatus : Byte {
    Grounded,
    Jumping,
    Dead
}

component Character : 1001 {
    FP Health;
    FP MaxHealth;
    PlayerStatus Status;
    array<FP>[4] Cooldowns;
}

event HitEffect : 1000001 {
    FPVector3 Position;
    Byte MaterialType;
}

synced_event PlayerEliminated : 1000002 {
    entity_ref Victim;
    entity_ref Killer;
}

signal OnMatchEnded(Int32 winningTeam);
```

Supported declarations:

| Declaration | Purpose |
| --- | --- |
| `struct` | Plain deterministic value; can be nested in components |
| `enum` / `flags` | Byte/SByte/Int16..UInt64-backed enumerations |
| `union` | Tagged union with a one-byte tag and the largest variant payload |
| `component : id` | ECS component with a fixed wire id (1000–999,999) |
| `input` | Per-tick input struct with packed button flags |
| `global` | Per-match global state |
| `event : id` / `synced_event : id` | RPC payloads (1,000,000–1,999,999) |
| `signal Name(args);` | Declared signal with deterministic `SignalIndex` |
| `asset Name;` | `asset_ref<T>` target declaration |

Field types: `FP`, `Byte`, `SByte`, `Int16`, `UInt16`, `Int32`, `UInt32`,
`Int64`, `UInt64`, `Boolean`, `Char`, `FPVector2`, `FPVector3`,
`FPQuaternion`, `FPMatrix4x4`, `FPAABB`, `FPBounds`, `entity_ref`,
`player_ref`, `QString<N>`, `asset_ref<T>`, `array<T>[N]`, `list<T>[N]`,
`dictionary<K,V>[N]`, plus user structs/enums/unions. Every value has a fixed
wire size; managed references (`float`, `string`, `List<T>`, ...) are rejected
with `CYGD001`–`CYGD004`.

Compile and validate:

```bash
# once
dotnet build sdk/unity/Tools~/CYG.Dsl.Cli/CYG.Dsl.Cli.csproj -c Release

# validate without writing files
dotnet sdk/unity/Tools~/CYG.Dsl.Cli/bin/Release/net10.0/CYG.Dsl.Cli.dll \
  Assets/Game.cyg --check

# emit C# next to the input (or use --out / --namespace)
dotnet sdk/unity/Tools~/CYG.Dsl.Cli/bin/Release/net10.0/CYG.Dsl.Cli.dll \
  Assets/Game.cyg --out Assets/CYGGenerated
```

The emitter refuses to write output when any diagnostic is an error. Component
ids and signal indices are part of the wire contract: keep them stable, never
reuse a retired id, and bump `SchemaHash` implicitly by changing the field
layout. Changing a component layout without bumping its explicit id is allowed
(the schema hash changes), but both peers must run the same generated code.

### ECS world, snapshots and rollback

`CYGWorld` stores generated components in dense per-type stores keyed by entity
slot. Entity refs carry a slot generation, so recycled slots never revive stale
references. Register the component types once, then drive the world through
`CYGWorldRunner`:

```csharp
using CookYourGame.Sim;

var world = new CYGWorld();
world.Register<Character>();
world.Register<Transform>();

CYGEntityRef player = world.CreateEntity();
world.Set(player, new Character { Health = FP.FromInt(100) });
world.Set(player, new Transform { Position = FPVector3.Zero });

var runner = new CYGWorldRunner(world, rollbackCapacity: 64);
runner.Tick(w =>
{
    // advance simulation; the post-tick state is snapshotted automatically
});
runner.Rollback(1); // restore and re-simulate the previous frame

uint checksum = world.ComputeChecksum(); // stores hashed in type-id order
CYGWorldSnapshot snapshot = world.CreateSnapshot(); // buffers are reusable
world.Restore(snapshot);
```

Snapshots are validated on restore (store count, type id and schema hash) so a
stale or mismatched snapshot fails fast instead of corrupting the world.
`CYGWorldRunner` snapshots after each tick, which means rolling back N frames
restores the world exactly as it was at the end of that tick. These are pure
C# types; they do not depend on `UnityEngine`, so the same code runs in the
editor, player builds and server-side simulations.

---

## 📄 License
Apache 2.0 — see [`LICENSE.md`](LICENSE.md). Copyright (c) 2026 CookYourGame Team.
