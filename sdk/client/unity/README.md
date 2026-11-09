# CYG Unity SDK

The Unity client SDK. Same surface and wire contract as the web SDK
(`cyg.js`), so a Unity player and a browser player can share a room.

CYG is server-authoritative: the simulation lives on the server, Unity renders
and predicts. Nothing here runs game logic locally.

## Install

Copy `Runtime/` into your Unity project's `Assets/`, or add this folder as a
local UPM package. Requires Unity 2022.3+ (uses `ClientWebSocket` and
`System.Text.Json`).

## Two things, not one

The SDK has two halves, because a game has two kinds of traffic:

| | what | why separate |
|---|---|---|
| `CygClient` | join, input, deltas, voice | has to be fast and small; a stale input is worse than a missing one |
| `CygRest` | wallet, inventory, leaderboard, quests, storage, IAP | a request and a reply; a dropped packet is a retry, not a desync |

Using `CygClient` for a wallet read would put a 200-byte answer on the game
lane. Using `CygRest` for movement would put every input behind a TCP retransmit.

## Join

```csharp
using CYG;

var room = await CygClient.JoinOrCreate("http://127.0.0.1:8297", "game_room");

room.OnRawDelta += (delta) => {
    var rows = CygDeltaDecoder.Decode(delta, MySchema.TagFields);
    foreach (var row in rows) ApplyToScene(row);
};

room.SendInput(BuildInputPacket());   // 20-byte movement frame, unreliable lane
room.Leave();
```

## Which lane? (`[transports]`)

The host opens lanes in its `config.toml`; the client picks one that is open.

```toml
[transports]
webrtc    = true    # browser default
websocket = true    # browser fallback, and the only lane WebGL can use
udp       = false   # native plugins: desktop, console
quic      = false
```

```csharp
await CygClient.JoinOrCreate(origin, room);                              // Auto (recommended)
await CygClient.JoinOrCreate(origin, room, null, CygClient.Lane.Udp);        // desktop/console
await CygClient.JoinOrCreate(origin, room, null, CygClient.Lane.WebSocket);  // WebGL, WeChat
```

`Auto` prefers UDP — a dropped packet costs one input sample instead of
stalling the stream behind a retransmit — and falls back to WebSocket.

**If you target WebGL, `websocket` must be on.** A WebGL build gets no UDP
socket from the browser at all, and `Auto` reads `UNITY_WEBGL` to know that.
WeChat hands the game a socket it only lets you talk over WebSocket, so it needs
the same lane.

Both lanes speak the same frames. Only the seat handshake and the framing
differ; input, messages, voice and delta decode are identical.

## The REST half

```csharp
var auth = new CygAuth(origin, serverKey);   // serverKey only needed to create accounts
await auth.LoginDevice(SystemInfo.deviceUniqueIdentifier);

var rest = new CygRest(origin, auth.Token);
```

### Wallet and inventory

```csharp
double gems = await rest.GetBalanceAsync("gems");
var wallet  = await rest.GetWalletAsync();              // Dictionary<string, double>
var items   = await rest.GetInventoryAsync();
int  arrows = await rest.GetItemCountAsync("arrow");
await rest.ConsumeItemAsync("arrow", 1);                // refused when not held
```

### Leaderboard

```csharp
// Idempotent: declare your boards at boot, every launch.
await rest.DefineBoardAsync("weekly_kills", "Weekly kills", sortOrder: -1);

await rest.SubmitScoreAsync("weekly_kills", 42);
var top  = await rest.ListScoresAsync("weekly_kills", 10);
var mine = await rest.GetMyScoreAsync("weekly_kills");   // null if never scored
```

A board that was never defined answers **404**, not an empty list. The SDK
passes that through — a game showing "no scores yet" for a mistyped board id
would never find out.

### Quests, notifications, cloud save

```csharp
var quests = await rest.GetQuestsAsync();
await rest.ClaimQuestAsync("daily_1");
await rest.TrackEventAsync("kill", 1);        // server-authoritative games send this from the room

var notes = await rest.GetNotificationsAsync(20);
await rest.DeleteNotificationAsync(id);

string save = await rest.ReadStorageAsync("slot_1");   // null on first launch
await rest.WriteStorageAsync("slot_1", json);
await rest.DeleteStorageAsync("slot_1");
```

`ReadStorageAsync` returns `null` for a slot that does not exist, so the first
launch is not an exception path.

### Purchases

```csharp
await rest.VerifyAppleAsync(receipt, productId);
await rest.VerifyGoogleAsync(receipt, productId);
```

A rewarded purchase usually pays out through a server rule or hook, so read the
wallet afterwards rather than trusting a local guess.

### Errors

```csharp
try { await rest.GetWalletAsync(); }
catch (CygApiException e) when (e.IsUnauthorized) { /* re-authenticate */ }
catch (CygApiException e) when (e.IsNotFound)     { /* wrong id, or not yours */ }
catch (CygApiException e) when (e.IsRefused)      { /* a rule, a quota, a balance */ }
```

## Auth

```csharp
await auth.LoginDevice(deviceId);                  // creates the account when the id is new
await auth.LoginEmail(email, password);
await auth.RegisterEmail(email, password, username);
```

Account creation is server-to-server: a client that has never signed in must
present the project's `control.session.server_key` as `CygAuth`'s second
argument. A game that only signs existing players in can leave it `null`.

The device id must be **10–128 bytes**. A shorter one is refused as a bad
credential, not as a bad request.

## State classes

`cyg schema-codegen --lang cs` emits `CYGSchemaGenerated.cs` — typed state
classes whose field order matches the wire rows. `CygDeltaDecoder` maps decoded
columns onto those classes; feed it the tag→field-names map from the generated
constants.

## Voice (relay-only)

The SDK owns the voice **wire framing, team/squad routing and mute/deafen
control**. Mic capture, Opus encode/decode and playback stay with your game
(Unity's `Microphone` class plus your Opus codec). There is no WebRTC mesh —
every speaker comes back through the authoritative server relay.

```csharp
var voice = room.AttachVoice();
voice.Enable();

voice.SetTeam("red");           // string or number — hashed to a u32
voice.SetMuted(true);           // host relays mic mute to peers
voice.SetDeafened(false);       // stop receiving everyone

voice.OnPacket += (speakerId, packet) => {
    // packet = 15-byte header + Opus payload
    var opus = new byte[packet.Length - 15];
    Buffer.BlockCopy(packet, 15, opus, 0, opus.Length);
    PlayOpus(speakerId, opus);
};

voice.Send(VoiceChannel.TeamOnly, seq, timestamp, flags, opusFrame);
// 0 = proximity · 1 = global · 2 = team-only · 3 = whisper

voice.Dispose();
```

Team ids are hashed with FNV-1a into a u32, matching the web (`schemaFnv1a`)
and Godot (`CYGCodec.fnv1a`) SDKs, so a label routes to the same bucket
everywhere. Set `null` (or call `LeaveTeam()`) to leave the squad.

## Wire notes

- matchmake: `POST /api/matchmake/joinOrCreate/{room}` → seat token, `gamePort`, `udpPort`
- join (UDP): `[0x20][len][seatToken]`; bind confirmation comes back as `[0x0a]`
  and carries the player id
- join (WebSocket): upgrade to `ws://host:{gamePort}/{room}?sessionId={id}`
- movement: `[op][seq u16 LE][20B]` — unreliable newest-wins, 20-byte payload
- state deltas: TLV rows `[tag][len][id u64][fields]` — decode with `CygDeltaDecoder`
- keep-alive: SDK pings every 3 s; idle >10 s drops the seat
- voice team: `[0x56]['T'][hash u32 BE]` — `0` leaves the team
- voice mute/deafen: `[0x56]['M'|'D'][1/0]`
- voice frame: `[0x56][sender u32 BE][seq u16 BE][timestamp u32 BE][mode][flags][len u16 BE][opus…]`

`udpPort` is `0` when the host has `udp = false`. The player id arrives in the
UDP join reply; a WebSocket client learns it from room state instead.

## Testing

`Tools~/CygRuntime.Tests` is a plain `net8.0` console project — no Unity needed.

```bash
cd Tools~/CygRuntime.Tests
dotnet run -c Release                                       # unit checks only
CYG_E2E_ORIGIN=http://127.0.0.1:8297 \
CYG_SERVER_KEY=<control.session.server_key> \
  dotnet run -c Release                                     # + live REST and both lanes
```

The live suite reports a lane the host did not open as **skipped**, not failed —
`udp = false` is a supported configuration.

## Status

- ✔ matchmake/join/bind over UDP **and** WebSocket, ping keep-alive
- ✔ movement input lane (20-byte frame, plus batched `0x26`)
- ✔ delta receive + TLV decode
- ✔ voice control + send + receive — relay-only
- ✔ REST: wallet, inventory, leaderboard, quest, notification, storage, IAP
- ✔ WebGL-safe lane selection
- ○ typed app messages (`send`/`request` RPC) — transport framing ready on both
  lanes; the server's `OP_APP` lane is the next wire addition
- ○ room-change redirect when matchmaking overflows
