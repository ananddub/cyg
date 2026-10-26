# CookYourGame Networking for Unreal Engine

Room-based multiplayer client for the CookYourGame engine. The plugin speaks the
same wire protocol as the Unity client, so a room hosts mixed Unreal and Unity
players without a translator.

Everything here is written against the engine's own modules — `WebSockets`,
`HTTP`, `Json`, `Core`, `Engine`, and for the tooling half `Slate` and
`PropertyEditor` — so one source build links on Win64, Linux,
Mac, iOS and Android. No third-party static or dynamic library is required, and
none is referenced.

## Install

Copy `CookYourGame/` into your project's `Plugins/` folder and regenerate the
project files:

```bash
UnrealBuildTool/Build/Linux/YourProjectEditor.Target.cs   # or right-click the .uproject -> Generate Project Files
```

`UCYGSubsystem` starts with the game instance and pumps the socket every frame,
so no manual tick or level blueprint is needed.

## Quick start

```cpp
#include "Core/CYGSubsystem.h"
#include "Core/CYGClient.h"

void UMyGameMode::BeginPlay()
{
    UCYGClient* Client = UCYGSubsystem::Get(this)->GetClient();

    Client->OnConnected.AddDynamic(this, &UMyGameMode::HandleConnected);
    Client->OnRoomEvent.AddDynamic(this, &UMyGameMode::HandleRoomEvent);
    Client->OnDisconnected.AddDynamic(this, &UMyGameMode::HandleDisconnected);

    Client->ConnectToRoom(TEXT("lobby"), TEXT("Player"));
}
```

Blueprint: `UCYGSubsystem::Get(WorldContextObject)` → `GetClient()` → bind the
events → `ConnectToRoom`.

## Configuration

`Project Settings → CookYourGame Networking`, or the ini block:

```ini
[/Script/CookYourGame.CYGSettings]
ServerHttpUrl=http://localhost:7777
ServerWsPath=/ws
MatchmakeMethod=joinOrCreate
MaxPlayers=8
```

## What is in this folder

| Path | Contents |
| --- | --- |
| `Public/Core` | opcodes, player record, settings, `UCYGClient`, `UCYGSubsystem` |
| `Public/Serialization` | `FCYGBitWriter` / `FCYGBitReader` (LEB128, zig-zag, half floats, smallest-three quaternions), `FCYGDeltaPacker` byte delta |
| `Public/Transport` | packet framing, wire-code translation, `FCYGWebSocketTransport`, `FCYGProfilingTransport` (the counting wrapper) |
| `Public/Gameplay` | `UCYGView`, `UCYGBehaviour`, `FCYGSyncVar`, `FCYGSyncList`, `FCYGPropertyMap`, `FCYGClassRegistry`, `UCYGActorManager`, `FCYGActorPool` / `ICYGPooledActor`, `UCYGSceneManager`, `UCYGAssetRegistry`, `UCYGEffects`, the movement, physics, animator and attachment components, the ownership and server toggles, `UCYGVisibility`, `FCYGCoordinateSpace`, `FCYGPoseInterpolator` |
| `Public/Netcode` | `FCYGRpcRouter`, `UCYGRpcComponent`, `FCYGLagCompensation`, `UCYGRollbackBuffer`, `CYGRewindMath` |
| `Public/BaaS` | `FCYGBaasHttp`, `FCYGBaasAuth`, `FCYGBaasStorage`, `FCYGBaasLeaderboard`, `FCYGBaasInventory`, `FCYGBaasChat`, `UCYGBaasLibrary` |
| `Public/Resilience` | `FCYGSessionCookie`, the bounded seat resume loop |
| `Public/Profiler` | `FCYGNetworkProfiler`, the rolling traffic ring the editor reads |
| `Public/Async` | `UCYGAsyncJoinRoom`, `UCYGAsyncBaasCall`, `UCYGAsyncAccountLogin` — the Blueprint join, login and account-call nodes |
| `../CookYourGameEditor` | editor-only module: the traffic tab and the details-panel readouts. No UObject, no generated header |

## State sync

A `UCYGView` owns the observed `UCYGBehaviour` components of its actor and sends
one delta per frame for each component that changed.

```cpp
// In a UCYGBehaviour subclass.
TSharedPtr<FCYGSyncVar<FVector>> Position;
TSharedPtr<FCYGSyncList<int32>>  Scoreboard;

virtual void BeginPlay() override
{
    Position   = MakeShared<FCYGSyncVar<FVector>>();
    Scoreboard = MakeShared<FCYGSyncList<int32>>();

    RegisterSyncVars({ Position, Scoreboard });

    Scoreboard->OnItemAdded([](int32 Index, const int32& Value) { /* ... */ });
}

void Jump()
{
    if (HasAuthority())
    {
        Position->Set(Position->Get() + FVector(0.f, 0.f, 400.f));
    }
}
```

Registration order is the wire order: a var's index is assigned once and never
renumbered, so both sides must declare the same list in the same sequence.
`SendIntervalSeconds` throttles a single var; `Set()` marks it dirty and a
remote `ApplyRemote()` fires the `OnChanged` hook.

The delta body is `[byte dirtyMask]` followed by one blob per set bit, so an
observed component that did not change costs nothing but a bit. A behaviour
blob is `[dirtyCount]([varIndex u8][value per codec])*`; a sync list block is
`[changeCount u8]([op u8]([index i32][item])*)`.

### Byte delta

`FCYGDeltaPacker::CreateDelta` diffs the assembled delta against the last frame
the same view sent, and `TryApplyDelta` replays it on the receiver. Each peer
keeps its own copy of the previous frame, so nothing has to be acknowledged:
if the two copies ever diverge, `TryApplyDelta` fails, and the next frame goes
out whole. The codec is the same one the Unity client ships, so a delta made in
either engine decodes in the other.

Call `FCYGDeltaPacker::RunSelfTest` from a console command or a boot check to
re-verify the round trip in a built game; it reports over 200 cases and touches
no network.

## RPC

`UCYGRpcComponent` lists the `UFUNCTION`s on the owning actor that may receive
calls in `RpcMethodNames`, binds them on register and unbinds on unregister.
The wire header is `[viewId u32 big-endian][method hash u16][target u8]`, where
the hash is FNV-1a 16-bit over the method name; arguments follow raw, and an
argument-less call still writes one byte so a truncated frame stays visible.

```cpp
// Owning actor.
UFUNCTION() void OnScore(FString Name, int32 Value);

// Anywhere holding the view.
FCYGRpcRouter::Bind(this, View, { TEXT("OnScore") });
FCYGRpcRouter::Send(View, TEXT("OnScore"), ECYGRPCTarget::Others, Args);
```

## Coordinates

The wire carries Unity's axes and metres: X right, Y up, Z forward. Unreal is
X forward, Y right, Z up and counts centimetres, so every world pose passes
through `FCYGCoordinateSpace` on its way on and off the wire. Both engines are
left-handed, which makes the difference a relabelling of the three components
and never a sign flip: an Unreal-only room is unaffected and a mixed room agrees
about where things are. Only placement converts. A vector that is a payload,
such as the direction in an RPC's arguments or an angular velocity, stays
engine-local.

## Movement, physics and animation

`UCYGTransformSync` is the base for everything that moves. The authority samples
on its own timer and writes `[byte flags]` followed by packed position, packed
rotation and scale, with position 1, rotation 2, scale 4 and teleport 8 as the
flag bits. That blob is byte-for-byte the one the Unity component emits, so a
Unity client reads an Unreal actor's state and the other way round.

A proxy keeps the incoming samples in a 32-frame ring and draws the world 100 ms
in the past: positions go through a cubic Hermite spline built from the sampled
velocities, rotations through a normalised lerp. Past the newest sample it
coasts along the last velocity for at most 200 ms. A move over 5 m is treated as
a teleport, which empties the ring, restarts it at the new pose and calls
`OnPoseTeleported` so the game can drop whatever smoothing it added itself.

`UCYGCharacterMovementSync` adds the part a character needs: a proxy switches
off its `UCharacterMovementComponent` tick so prediction stops fighting the
incoming state, and writes the interpolated velocity back so a landing or a wall
push still reads correctly.

`UCYGPhysicsSync` covers a simulating `UPrimitiveComponent` and syncs pose plus
both velocities, correcting hard past two metres of error. Angular velocity is a
pseudo-vector, so it is relabelled but not unit-converted. Its blob is
Unreal-specific: give a Unity actor the same authored layout, or let Unreal spawn
it.

`UCYGAnimatorSync` takes an explicit `Parameters` list of names and types rather
than polling the animation blueprint, and reads and writes the values through
`FProperty`. Triggers are edge events, so they only travel when the game calls
`FireTrigger`. A delta is `[byte flags][byte count]([byte index][value])*` plus
an optional trigger list, so one changed parameter costs three bytes of framing
over its value; like physics, the layout is engine-local.

## Mounts, culling and who runs what

`UCYGAttachmentSync` keeps a mounted actor attached to the networked actor it
rides: a weapon in a hand, a driver in a vehicle, a crate standing on a moving
platform. It rides the view's delta channel, so no opcode is involved, and its
blob is the one the Unity hierarchy component writes: `[byte flags]` with parent
1, scale 2 and child order 4, then `[varint parentViewId]`, packed relative
location, relative rotation, scale as three singles, and the order byte. It is
the only behaviour that deliberately skips the sync-var header, because a Unity
mount sends no header and one extra byte would shift every field after it; that
also means a subclass here must not register vars.

The parent travels as a view id and the pose is measured against that view's
actor root, which is the basis a Unity peer measures from too. An offset authored
against a hand socket therefore goes out as a relative location rather than as a
socket name the far side may not have. A child order number has no Unreal
equivalent, so bit 4 is read and dropped. While the mount holds still, the pose
stays under 0.01 cm and 0.05 degrees and nothing is sent. A proxy whose parent
has not spawned yet queues one patch and waits up to sixty ticks for it, giving
up early and quietly if that actor is despawned instead; mounting an actor inside
itself is refused, and so is a `Target` whose mobility is not `Movable`, because
the engine will not reparent something static at run time. The component occupies
one of the view's eight dirty-mask bits.

`UCYGOwnershipToggle` and `UCYGServerToggle` send nothing at all. Each holds two
component lists and turns one on while the other off: the first keyed on
`IsMine`, the second on leading the room, optionally also counting a view this
peer owns as authority. Being off means neither ticking nor activation, since a
simulation has to stop ticking while a camera or an audio player has to stop
being the active one. Neither registers with its view, so the pair costs zero
wire bytes. The role component polls, because the master client changes with the
roster and no event says so; the change check keeps that one bool compare per
frame in a settled room.

`UCYGVisibility` stops drawing what the local player cannot see. Rules are
`FCYGVisibilityRule` data, ANDed together: a distance rule with an entry radius
and a wider exit radius, so an actor sitting on the boundary changes state once
per hysteresis band, and a grid rule comparing the cell and its neighbours, on
the ground plane or with height counted. Distances are centimetres, so Unity's
50 metres is this rule's 5000, and the height axis is Z here where the Unity
client reads Y. Evaluation runs on a quarter-second interval and fails open: no
rules, no root, or no camera means everything is drawn, because the mistake worth
avoiding is an empty world.

This is a presentation gate, not an interest filter. The room still sends and the
bytes still arrive; what gets switched off is this actor's primitives and its
pose interpolation, since an actor nobody can see has no reason to spend its
smoothing catching up to where it would have been. The culled state is published
through `FCYGVisibilityGate::ShouldReceive(viewId)` for a game that wants to skip
its own expensive delta handling, and nothing in the connection path reads it:
real traffic culling stays server-side, where the sender knows who could see.

## Lag compensation

`UCYGRollbackBuffer` records a body's bounds sixty times a second into 120
frames of history. `FCYGLagCompensation::RaycastRewind` rebuilds every registered
body at the moment a shot claims it was fired and keeps the nearest entry, which
is what an authoritative host validates a claim against; anything asking to
rewind more than 300 ms is refused outright. `SubmitHitClaim` is the shooter's
side of that, and `TryReadClaim` the host's.

The two shape tests live in `CYGRewindMath` and take the shape as data instead of
asking the physics scene, because the scene only ever holds the present. A body
that was missed by the history simply is not rewound, so a laggy hit fails rather
than landing on a guessed pose.

## Actor pooling

Spawn and destroy is the expensive part of a networked hot path: bullets, hit
effects and pickups live for a second or two each. `FCYGActorPool` recycles them.
`EnablePooling(Class, Capacity)` (or `UCYGActorManager::EnablePooling`, which
forwards to it) opts a class in, `Prewarm` builds the idle actors up front, and
from then on a spawn writes a transform instead of constructing an actor. The
manager routes both local and wire-driven spawns through the pool, and
`DestroyNetworked` retires an actor instead of destroying it while the idle list
has room; a full pool destroys, so an unbounded class cannot grow without limit.

A recycled actor never sees a second `BeginPlay`, which is the one thing a pooled
class has to know about. `ICYGPooledActor` is the substitute:
`OnCYGSpawnedFromPool` fires once the actor is placed and its view id is assigned,
`OnCYGReturnedToPool` before it goes idle. Both are `BlueprintNativeEvent`s, so a
blueprint implements either without any C++. Idle actors are hidden, un-collided
and un-ticked, and the pool clears them on a level change and when the link is
lost — parked actors belong to the room that made them.

## Scenes and effects

Neither of these needed a new opcode. Both ride the room-event channel
(`SendRoomEvent` / `OnRoomEvent`), and because both clients split a leading event
name off a relayed bytes message, a scene change raised in Unreal reaches a Unity
listener and the other way round.

`UCYGSceneManager::LoadNetworkedScene(MapName, Mode)` is master-client only: a peer
that loaded on its own would leave the room holding two maps. It broadcasts
`[string map name][byte mode]` as `CYG_SCENE` and then loads locally, and the peers
load when the room relays the event back to them. `Single` travels with `OpenLevel`
and reports through `OnNetworkSceneLoaded` when the per-frame pump sees the new
world; `Additive` uses `LoadStreamLevel` and answers immediately, skipping the work
when that map is already in. The mode numbers are Unity's `LoadSceneMode` values.

```cpp
UCYGSubsystem* Cyg = UCYGSubsystem::Get(World);
Cyg->GetSceneManager()->LoadNetworkedScene(TEXT("Arena_02"), ECYGSceneLoadMode::Single);
```

`UCYGEffects::PlaySoundAtLocation` and `PlayEffectAtLocation` fire a one-shot at a
position without spawning any actor at all. The body is
`[byte kind][u16 asset id][u32 sender actor]` followed by
`[packed position][float volume][float pitch]` for a sound or
`[packed position][quaternion][float scale]` for a particle. Positions convert to
wire axes the way every other pose does, the sender plays locally and drops the
echo the room sends back to it, and an unknown asset id logs instead of guessing.
Ids come from a `UCYGAssetRegistry` data asset shaped like `UCYGClassRegistry`, so
a renamed sound keeps its wire identity.

Particle playback pulls in `Niagara`, an Epic module rather than third-party
software; the plugin enables it in `.uplugin` and lists it in `Build.cs`. A game
that uses no effects can leave it off — nothing else in the plugin refers to it.

## Accounts and saved data

The service layer talks to the same control plane the room reservation came from,
over `FCYGBaasHttp` — the engine's own `FHttpModule` and `Json`, so nothing here
opens a socket or links a library. Every call is a callback, never a coroutine, and
the base url is `Project Settings → CookYourGame Networking → ServerHttpUrl` unless
`FCYGBaasHttp::BaseUrl` overrides it at boot.

```cpp
FCYGBaasAuth::LoginDevice([](bool bOk, const FCYGBaasSession& Session)
{
    if (!bOk) { return; }

    FCYGBaasStorage::Write(TEXT("progress"), TEXT("chapter"), TEXT("{\"cleared\":3}"),
        [](bool bWritten, const FString& /*Value*/) { /* ... */ });
});
```

| Surface | Endpoints | Notes |
| --- | --- | --- |
| `FCYGBaasAuth` | `POST /v2/account/authenticate/{device,email,custom}` | `create=true\|false` and an optional `username` go in the query, matching the Unity client |
| `FCYGBaasStorage` | `POST/PUT /v2/storage`, `PUT /v2/storage/delete`, `GET /v2/storage/{collection}` | values are JSON text; a write carries `permission_read`/`permission_write` |
| `FCYGBaasLeaderboard` | `POST/GET /v2/leaderboard/{id}` | what a submission does with a score is the server's decision, not asserted here |
| `FCYGBaasInventory` | `GET /v2/inventory`, `POST /v2/inventory/{equip,consume,grant}` | an operation is reported successful only after the refreshed cache came back |
| `FCYGBaasChat` | `GET /v2/channel/{id}`, `POST/PUT/DELETE /v2/channel/{id}/message[/{message_id}]` | a bounded local timeline; build the dotted id with `RoomChannelId`/`GroupChannelId`/`DirectChannelId`; `Send` wraps text as `{"text":…}`, `SendContent`/`EditContent` take raw JSON, `Ingest` accepts a live message |

An account is not a room: signing in needs no seat, and a seat needs no account.
`FCYGBaasAuth` publishes the bearer it obtained to `FCYGBaasHttp`, so service calls
are authenticated from the moment a login answers, and `Logout` clears both.

**Persistence caveat.** `deviceid` and `customid` logins are the whole of a
player's identity on this plane, and by default `FCYGBaasAuth::bPersistSession`
writes the token, the refresh token, the user id and the nickname to
`CookYourGameSession` in `GameUserSettings.ini` — plain text in a packaged build.
A game that ships a real account store should set it to `false` and keep the
session itself. `RestoreSession()` reads it back; `Logout()` erases it. The token
never appears in a log line, and `DeviceId()` is the stable per-install id generated
on first run and reused from the ini afterwards.

Blueprint reaches the same surface through `UCYGBaasLibrary` (read the id, sign out,
point at another farm) and two `UBlueprintAsyncActionBase` nodes:

- `CYG | Login Device (Async)`, `CYG | Login Email (Async)`, `CYG | Login Custom Id (Async)` —
  `OnSuccess(UserId, Username, Token)` / `OnFailure(Reason)`.
- `CYG | Account Call (Async)` — any verb against any path, `OnSuccess(StatusCode, Body)`
  for whatever a server answered (a `404` is an answer) and `OnFailure` only for a call
  that never reached one.

## Coming back to a dropped seat

A link that dies mid-match does not have to end it. The reservation outlives the
socket, so `FCYGSessionCookie` keeps the room and session it was given and paces a
bounded resume: 0.5 s, then 1, 2 and 4 (capped), at most six attempts inside a window
that closes thirty seconds after the drop. `UCYGSubsystem` drives it from the ticker
that already pumps the socket, so it keeps counting while the link is down — no
driver actor, and no second loop for a game to forget to tick.

```cpp
FCYGSessionCookie::WindowSeconds   = 45.f;   // default 30
FCYGSessionCookie::MaxAttempts     = 10;     // default 6
FCYGSessionCookie::MaxBackoffSeconds = 8.f;  // default 4
```

The resume goes back through `UCYGClient::ReconnectToSeat()`, which rebuilds the
url from the seat the client already holds, so the player returns with the same
actor number and the room keeps its views. Only an unscheduled drop arms the loop:
`Disconnect()` clears the cookie first, and shutdown detaches it before closing, so
leaving on purpose cannot be mistaken for a failure. A confirmed join retires the
loop, and a room that answers nothing is released after eight seconds rather than
parking the window on the attempt that never reported.

Three close reasons mean "stop trying": the host is shutting down, the room closed
with an error, or the plane itself refused to reconnect the seat.
`FCYGWireProtocol::IsTerminalClose` is what the loop reads for that, so a full room
or a dying process is never retried with the same token — and a rejected handshake
counts too, because the client's last reported error code is checked alongside the
close code that followed it. The cookie itself holds no credential: the token stays
with the client that was handed it, which is why `Describe()` is safe to print in any
log.

## Seeing what the wire is doing

`FCYGNetworkProfiler` is a static ring of sixty one-second windows: bytes and
packets each way, per-opcode totals, how many frames the pump ran and the longest
one. It is off unless the project asks for it, so a record call in a shipped build
is one branch that is never taken and the storage is allocated once, statically.

```ini
[/Script/CookYourGame.CYGSettings]
bProfileTraffic=true
```

Counting is installed where packets already are. When the sink is enabled,
`UCYGClient::EnsureTransport` wraps the websocket transport in
`FCYGProfilingTransport`, a decorator that forwards every call and re-raises every
event, then reads the canonical header of whatever passes through. Nothing in the
connection path knows measurement exists, so a game can inject its own transport,
and a game that never turns the sink on never pays for it. Because the wrapper is
chosen when the link is built, turning recording on mid-session starts it at the
next connect; turning it off stops it at once.

The window roll and the frame counts come from `UCYGSubsystem::PumpInbound`, which
runs whether or not the link is up — a drop is exactly when the tick history
matters, and it is also what makes the editor's resume loop visible.

### Editor surfaces

`CookYourGameEditor` is an editor-only module with no UObject in it, so it adds no
generated header of its own. It contributes:

* **Window → CookYourGame Traffic** — the room, actor number, master or peer, link
  state, round trip, live view count, then bytes and packets per second for the
  open window and the ring average, two block graphs of the last minute (out above
  in, both scaled to the same peak), and one line per opcode that moved anything.
  `Reset ring` forgets what was collected; `Toggle recording` flips the sink and
  writes the project setting.
* **A live row on `UCYGView`** — wire id, owner, whether this side owns it and may
  write it, then the observed components in registration order, which is also their
  order in the dirty mask, each with its sync var and dirty counts.
* **A live row on every `UCYGBehaviour`** — the view it resolved, authority, vars
  waiting to send, plus the per-class readout a designer actually tunes: the pose
  target and sample period, the physics body and its move/turn thresholds, the
  anim mesh and parameter count, the view a mount is currently riding. The two
  toggles get the same row too, where authority and mine are the whole answer
  they act on.

Each row is a text block whose value is read at paint time, and one 4 Hz ticker in
the module tells the blocks to repaint, so a panel that is not on screen costs
nothing and a closed tab leaves nothing behind. The readers take weak references: a
details panel outliving the component it was built for says nothing rather than
crashing.

The widget reads only the runtime module's public surface — `FCYGNetworkProfiler`,
`FCYGViewTable::Count`, `UCYGClient`'s getters, `UCYGView`'s authority tests and
`SyncVars()` — so there is no editor-only copy of any number to fall out of date.

One caveat worth knowing before an engine bump: this module is the only place the
plugin touches editor Slate, and a few of those names have moved between 5.x
minors (`STextBlock::Text_Lambda`, `SWidget::InvalidateLayoutAndVolatility`,
`FPropertyEditorModule`'s layout registration). Start there if a new engine
complains about the editor module. The toolbar button was left out on purpose,
because `FToolMenuEntry::InitToolBarButton` changed its parameter order across
minors and the Window menu entry needs no code at all.

## Blueprint

`CYG | Join Room (Async)` and `CYG | Matchmake (Async)` are
`UBlueprintAsyncActionBase` nodes with `OnSuccess` / `OnFailure` pins. They
wait for the joined-room event, fail on a server error or a dropped link, and
fail on a timeout when one is given.

## Wire format

Canonical packet: `[payload length u16 big-endian][opcode u8][payload]`.

On the socket a message is `[wire code u8][payload]`, except the game-layer
opcodes (rpc, rewind hit, spawn, despawn) which are tunneled as
`[17]['C'][canonical packet]` so the room can tell them apart from the opaque
blobs it also relays.

## License

Apache 2.0 — see [`LICENSE`](LICENSE). Copyright (c) 2026 CookYourGame Team.
