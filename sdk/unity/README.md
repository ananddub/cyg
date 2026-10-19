# 🍳 CookYourGame (CYG) Unity SDK

Ultra-lightweight, zero-allocation drag-and-drop multiplayer networking client for Unity.
Designed to replace legacy, expensive solutions (like reference PUN) with an ultra-affordable, high-performance architecture.

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
  Sim/             optional fixed-point simulation, rollback and input hub
  BaaS/            CYGAuth, CYGHttp, CYGStorage, CYGInventory, CYGLeaderboard,
                   CYGChat
Editor/
  CookYourGame.Editor.asmdef
  CYGProfilerWindow.cs
```

### What each area gives you

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

Bit-exact determinism has been verified by the included checksum tests on one
host. It has **not** been verified across Windows/Linux/macOS, different CPU
architectures, or Unity player builds. Do not ship a cross-platform lockstep
claim without running the same checksum suite on every target platform.

The input hub relays inputs; it does not ship state snapshots or a
server-driven tick epoch. Treat all peers as starting together at tick 0
(a loading/start barrier) and do not allow late join or mid-match reconnect
without a game-level resync. The current mode is a lockstep prototype, not a
drop-in replacement for a server-simulated reference room.

---

## 📄 License
MIT License — see [`LICENSE.md`](LICENSE.md). Copyright (c) 2026 CookYourGame Team.
