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

## 📦 What's Inside the SDK?

* **`CYGNetwork`**: Static singleton manager (Connect, Rooms, MasterClient, RPCs).
* **`CYGView`**: Network identity and ownership controller (`view.IsMine`).
* **`CYGTransformSync`**: Smooth dead-reckoning movement and rotation synchronization.
* **`CYGTransport`**: Thread-safe native WebSocket networking client.
* **`CYGRPC`**: Remote procedure call attribute (`[CYGRPC]`).
* **`CYGPlayer`**: Player data model (`ActorNumber`, `Username`, `IsMasterClient`).

---

## 📄 License
MIT License — CookYourGame Team.
