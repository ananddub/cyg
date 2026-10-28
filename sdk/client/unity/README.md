# CYG Unity SDK (runtime)

The Unity client SDK — same surface and wire contract as the web SDK
(`openoxide.js`), speaking the server's raw-UDP plane.

> This replaces the previous deterministic-lockstep SDK (fixed-point math,
> FP types, lockstep menus) — that architecture is gone. CYG is
> server-authoritative: simulation lives on the server, Unity renders and
> predicts.

## Install

Copy `Runtime/` into your Unity project's `Assets/` (or add this folder as a
local UPM package). Then:

```csharp
using CYG;

// join (matchmake over HTTP → seat → UDP join → bind)
var room = await CygClient.JoinOrCreate("http://127.0.0.1:8124", "game_room");

room.OnRawDelta += (delta) => {
    var rows = CygDeltaDecoder.Decode(delta, MySchema.TagFields);
    foreach (var row in rows) ApplyToScene(row);
};

room.SendInput(BuildInputPacket());   // 20-byte movement frame, unreliable lane
room.Leave();
```

## State classes

`ox-game schema-codegen --lang cs` emits `CYGSchemaGenerated.cs` — typed state
classes whose field order matches the wire rows. `CygDeltaDecoder` maps decoded
columns onto those classes; feed it the tag→field-names map from the generated
constants.

## Wire notes

- matchmake: `POST /api/matchmake/joinOrCreate/{room}` → seat token + game port
- join: `[0x20][len][seatToken]` over UDP; bind confirmation comes back
- movement: `[op][seq u16 LE][20B]` — unreliable newest-wins, 20-byte payload
- state deltas: TLV rows `[tag][len][id u64][fields]` — decode with `CygDeltaDecoder`
- keep-alive: SDK pings every 3 s; idle >10 s drops the seat

## Status

- ✔ matchmake/join/bind, ping keep-alive
- ✔ movement input lane (20-byte frame)
- ✔ delta receive + TLV decode (descriptor/generated-classes pairing)
- ○ typed app messages (`send`/`request` RPC) on the UDP plane — transport
  framing ready; the server's `OP_APP` lane is the next wire addition
  (the web SDK already has the full RPC surface today)
