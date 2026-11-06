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

## Voice (relay-only)

Unity rides the same UDP lane as movement. The SDK owns the voice **wire
framing, team/squad routing and mute/deafen control**; mic capture, Opus
encode/decode and playback stay with your game (Unity's `Microphone` class plus
your Opus codec). There is no WebRTC mesh here — every speaker comes back
through the authoritative server relay.

```csharp
var voice = room.AttachVoice();
voice.Enable();

voice.SetTeam("red");           // string or number — hashed to a u32
voice.SetMuted(true);           // host relaying mic mute to peers
voice.SetDeafened(false);       // stop receiving everyone

voice.OnPacket += (speakerId, packet) => {
    // packet = 15-byte header + Opus payload; decode and play it
    var opus = new byte[packet.Length - 15];
    Buffer.BlockCopy(packet, 15, opus, 0, opus.Length);
    PlayOpus(speakerId, opus);
};

// Stream one captured/encoded frame:
voice.Send(VoiceChannel.TeamOnly, seq, timestamp, flags, opusFrame);
// 0 = proximity · 1 = global · 2 = team-only · 3 = whisper

voice.Dispose();                // detaches the packet handler
```

Team ids are hashed with FNV-1a into a u32, matching the web
(`schemaFnv1a`) and Godot (`CYGCodec.fnv1a`) SDKs, so a label routes to the
same bucket everywhere and the per-packet compare stays an integer compare.
Set `null` (or call `LeaveTeam()`) to leave the squad.

## Wire notes

- matchmake: `POST /api/matchmake/joinOrCreate/{room}` → seat token + game port
- join: `[0x20][len][seatToken]` over UDP; bind confirmation comes back
- movement: `[op][seq u16 LE][20B]` — unreliable newest-wins, 20-byte payload
- state deltas: TLV rows `[tag][len][id u64][fields]` — decode with `CygDeltaDecoder`
- keep-alive: SDK pings every 3 s; idle >10 s drops the seat
- voice team: `[0x56]['T'][hash u32 BE]` — `0` leaves the team
- voice mute/deafen: `[0x56]['M'|'D'][1/0]`
- voice frame: `[0x56][sender u32 BE][seq u16 BE][timestamp u32 BE][mode][flags][len u16 BE][opus…]`
  — the host overwrites the sender slot when relaying, so `OnVoice` reports the
  real speaker

## Status

- ✔ matchmake/join/bind, ping keep-alive
- ✔ movement input lane (20-byte frame)
- ✔ delta receive + TLV decode (descriptor/generated-classes pairing)
- ✔ voice control + send + receive (`AttachVoice`, `SetTeam`, `Send`, `OnPacket`)
  — relay-only; mic capture / Opus codec / playback is game-side
- ○ typed app messages (`send`/`request` RPC) on the UDP plane — transport
  framing ready; the server's `OP_APP` lane is the next wire addition
  (the web SDK already has the full RPC surface today)
