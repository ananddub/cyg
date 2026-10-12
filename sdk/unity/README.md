# OpenOxide Gaming SDK for Unity

High-performance, sub-millisecond, low-latency UDP multiplayer client SDK for Unity (2021.3+ / Unity 6).

## Features
- **Raw UDP & WebSocket Networking**: Zero-allocation binary packet serialization.
- **Authoritative Server Sync**: 60Hz delta alignment with lag compensation.
- **Packed 20-Byte Input Streaming**: Low bandwidth footprint.
- **Match Replay (.oxr)**: Record matches and play back with scrubber / instant kill-cams.
- **Native Spectator Mode**: Observer streams without player entities.

## Installation via UPM (Unity Package Manager)
Add via Git URL or local folder:
```json
"com.openoxide.sdk": "file:../../sdk/unity"
```

## Quick Start
```csharp
using UnityEngine;
using OpenOxide;

public class GameNetwork : MonoBehaviour
{
    private OpenOxideClient client;
    private OpenOxideRoom room;

    void Start()
    {
        client = new OpenOxideClient("127.0.0.1", 7778);
        room = client.JoinOrCreate("arena");

        room.OnDelta += delta => {
            Debug.Log($"Received {delta.Entities.Length} combatants");
        };
    }

    void Update()
    {
        room?.Update();

        // Send 20-byte packed player inputs
        room?.SendInput(flags: 16, aimYaw: 0f, aimPitch: 0f, moveX: 0f, moveZ: 1f);
    }

    void OnDestroy()
    {
        room?.Leave();
    }
}
```
