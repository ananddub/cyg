# CookYourGame SDK for Godot 4 (`CYGGodot`)

Network client for the OpenOxide Gaming room relay: view-based state sync, RPCs,
Netfox-style rollback, lag compensation, the `/v2/` BaaS surface and an editor
dock — in GDScript, with no native plugin and no engine fork.

Everything lives under `addons/cookyourgame/`. Design derives from Netfox's
MIT-licensed rollback stack and Mirror's transport/metrics layer (attribution:
`addons/cookyourgame/LICENSE-NETFOX.md` and
`addons/cookyourgame/LICENSE-MIRROR.md`). The code is an independent GDScript
reimplementation written against this repository's Rust relay; no upstream
source text is copied.

## Install

1. Copy `addons/cookyourgame/` into your project's `addons/` directory.
2. Project → Project Settings → Plugins → enable **CookYourGame**.
3. Enabling registers the `CYGNetwork` autoload; disabling removes it. The
   bottom dock **CYG Network** appears in the editor as soon as the plugin is on.

Requires Godot 4.7 or newer (the code uses `class_name`-registered statics and
typed arrays throughout).

## What talks to what

```
your scene
  └── CYGView ── observable components ── serialization ── core/CYGNetwork (autoload)
                                                            │
                                                            ├── core/CYGClient ── WebSocketPeer
                                                            └── rollback/CYGNetworkTime ── tick clock
```

The relay is the room server on `:7777` (the same port answers the matchmake
`/api/v1/games/.../matchmake` POST and the room WebSocket). BaaS calls go to the
engine's `/v2/` control plane on `web_port` (8090 by default).

## Layers

| Layer | Classes | Role |
| --- | --- | --- |
| `core/` | `CYGNetwork`, `CYGClient`, `CYGMultiplayerPeer`, `CYGOpcodes`, `CYGTypes`, `CYGPlayer`, `CYGPrefabEntry`, `CYGPrefabRegistry`, `CYGSceneManager`, `CYGServerSettings` | connection, seat, opcode routing, roster, prefabs, ticks |
| `components/` | `CYGView`, `CYGBehaviour`, `CYGActor2D/3D`, `CYGTransformSync2D/3D`, `CYGPhysicsSync2D/3D`, `CYGCharacterSync2D/3D`, `CYGAnimationSync`, `CYGBoneAttachmentSync`, `CYGOwnershipToggle`, `CYGAuthorityToggle`, `CYGVisibilityCull` | the things you attach to a scene node |
| `serialization/` | `CYGBitPacker`, `CYGBitReader`, `CYGDeltaPacker`, `CYGPropertyMap`, `CYGSyncProperty`, `CYGSyncArray`, `CYGSyncDict`, `CYGRpcRouter` | the wire format, packed bitwise with zig-zag varints |
| `rollback/` | `CYGNetworkTime`, `CYGHistoryBuffer`, `CYGRollbackSynchronizer`, `CYGPredictiveSync`, `CYGTickInterpolator` | prediction, rewind, replay, smoothing |
| `resilience/` | `CYGSessionCookie`, `CYGNodePool`, `CYGLagComp2D/3D`, `CYGLagHitbox2D/3D` | identity across drops, pooling, rewind hit tests |
| `profiler/` | `CYGNetworkProfiler`, `CYGProfilingTransport`, `CYGProfilerHUD` | per-second ring of bytes, packets, ticks, RTT, mispredicts |
| `events/` | `CYGNetworkAudio2D/3D`, `CYGNetworkParticles2D/3D` | one-shot effects that dedup and pool instead of spawning |
| `baas/` | `CYGHttp`, `CYGAuth`, `CYGStorage`, `CYGInventory`, `CYGLeaderboard`, `CYGChat` | the `/v2/` REST surface |
| `editor/` | `cyg_editor_dock.gd`, `CYGSceneScanner` | live stats and a scene audit |

## Quick start

```gdscript
extends Node3D

func _ready() -> void:
	CYGNetwork.joined_room.connect(_on_joined)
	CYGNetwork.player_joined.connect(_on_player)
	CYGNetwork.connect_to_room("arena", "Ada")   # or join_or_create_room(...)

func _on_joined(room: String) -> void:
	print("in %s as actor %d" % [room, CYGNetwork.local_actor_number])

func _on_player(player: CYGPlayer) -> void:
	# `player` is a roster entry; spawn their avatar from a registered prefab.
	CYGNetwork.spawn_prefab("avatar", Vector3.ZERO)
```

A networked object is a `CYGView` plus observable components, in the order they
should appear on the wire:

```gdscript
# player_root (CharacterBody3D)
#   ├── CYGView                  (this node's identity: view_id + owner)
#   ├── CYGTransformSync3D       (observable 0)
#   └── CYGCharacterSync3D       (observable 1: predicts on the owner, rewinds elsewhere)

func _physics_process(delta: float) -> void:
	# Nothing to send by hand: only the owner marks a component dirty, and
	# `CYGNetwork` packs one delta packet per view on every `network_tick`.
	# Proxies apply what arrived and interpolate the rest.
```

RPCs are named, not annotated, because the relay has no compiler to hook:

```gdscript
view.bind_rpc("footstep", _on_footstep)          # receiver
view.send_rpc("footstep", [pos], CYGOpcodes.Target.ALL)   # sender
```

`CYGOpcodes.Target` picks the audience: `ALL`, `Owner`, `Others`, or a specific
actor. `server_rpc()` is the same path aimed at the room's own listener.

## BaaS

`CYGHttp` is the single transport: base URL, the one `Authorization` header, the
retry-once-on-connection-failure policy and the `Response` object. The rest of
the layer drives `CYGHttp.send_method()`, so there is one place where
credentials and endpoints are decided.

```gdscript
CYGAuth.login_device(func(result: Dictionary) -> void:
	if result["ok"]:
		CYGStorage.set_value("saves", "slot1", save_dict, func(r: Dictionary) -> void:
			print(r["ok"], " ", r["error"]))
```

* `CYGAuth` — guest (`OS.get_unique_id()`), email and custom-key login, refresh,
  restore from `user://cyg_auth_session.cfg`, and publishing the token to
  `CYGHttp` so every other class is authenticated.
* `CYGStorage` — documents, collections, versioned writes, list with cursor.
* `CYGInventory` — grant, define, equip to a slot, consume, instance ids.
* `CYGLeaderboard` — weekly boards, `int64` scores normalised from strings.
* `CYGChat` — channels, history paging, moderation. Channel ids are the
  server's dotted form; use `CYGChat.room_channel("lobby")`,
  `CYGChat.group_channel(group_id)` or `CYGChat.direct_channel(a, b)`.

`CYGHttp.server_key_credential()` sends `Authorization: Basic base64("<server
key>:")` for the routes the Rust gateway guards with a key instead of a session
(`inventory/define` has no `Authed` extractor); the other calls send the bearer
token. Never both — the gateway rejects a request whose `Authorization` count is
not exactly one.

## Resilience

`CYGNetwork` reconnects on its own after a drop that the game did not ask for:

* A close is consented when `disconnect_from_room()` caused it. Any other close
  (including the relay's own `4000 "leave"` reply, which cannot distinguish the
  two) schedules a retry unless `auto_reconnect` is off or no room was joined.
* Backoff is `0.5 s · 2^(attempt-1)` — 0.5, 1.0, 2.0 — for
  `CYGSessionCookie.MAX_RECONNECT_ATTEMPTS` (3) attempts, i.e. a 3.5 s outage
  window. Each attempt has a 3 s deadline of its own.
* The first attempt replays the server-issued `reconnectionToken`, which reclaims
  the seat, actor number and view state if the room still holds it. The first
  refusal retires the token, so a later attempt joins by room name and takes a
  fresh actor number.
* `reconnecting(attempt, delay_seconds)` fires per scheduled retry, `joined_room`
  again on the rejoin, and `room_error` when the budget is spent.
* `CYGSessionCookie` persists the client identity under `user://` and carries the
  retry budget across restarts. It is not a seat ticket: after a process restart
  only a join by room name is possible.

`CYGNodePool` bounds instantiation for spawns and effects; `CYGLagComp2D/3D` with
`CYGLagHitbox2D/3D` rewind raycasts against the tick-stamped poses each hitbox
recorded.

## Profiler and editor dock

```gdscript
var transport := CYGProfilingTransport.attach(CYGNetwork)   # wraps network.client
# ... play ...
var snap: Dictionary = CYGNetworkProfiler.active.snapshot()
print(snap["totals"]["bytes_sent"], " bytes, ", snap["rtt"]["avg_ms"], " ms rtt")
transport.detach()                                          # byte-identical to unwrapped
```

The wrapper counts every framed and wire packet in both directions with the
3-byte header excluded, attributes view-delta bytes per view id, and keeps a
60-window ring of one second each. `degrade_enabled` adds latency, jitter,
drops and duplicates for testing the rollback path without a bad network.
`CYGProfilerHUD` renders the same snapshot.

The **CYG Network** dock shows live connection state, tick rate, latency, view
and delta counters, and a **Scan Scene** button that runs `CYGSceneScanner` over
the edited scene and lists findings per node: `NO_ROOT`, `ORPHAN_VIEW`,
`UNTRANSFORMABLE_NODE`, `DUPLICATE_VIEW_ID`, `TOO_MANY_OBSERVABLES`,
`SILENT_VIEW`, `BROKEN_OBSERVABLE`, `OBSERVABLE_NOT_CHILD`, `PROVIDER_OBSERVED`,
`WRONG_BODY`, `DOUBLE_POSE_WRITER`, `NO_VIEW`.

## Deviations from the Unity and Unreal SDKs, and why

* **The transport is a WebSocket, not UDP.** This relay *is* the room server on
  `:7777`; nothing here speaks ENet or KCP. The 20-byte input packet and 60 Hz
  delta alignment of the other SDKs are preserved in the wire format, not in the
  socket type.
* **`CYGMultiplayerPeer` is a shim, not an engine peer.** Godot's
  `MultiplayerAPI` insists on owning the transport and on its own RPC codegen,
  neither of which composes with a relay that defines its own opcodes. The class
  presents the `put_packet`/`get_packet`/sender-id face — so `MultiplayerAPI`
  calls work — while every byte still crosses the CYG frame layer.
* **Lag compensation is peer-hosted.** The relay declares opcode 7 (`RewindHit`)
  and opcode 4 (`PlayerInput`) but never forwards them, so no server in this
  stack will roll back a shot for you. The shooter decides with a local rewind of
  the replica history it already has, sends the claim as an RPC, and the peer that
  owns the target's view re-runs the rewind as the veto.
* **No server authority.** The room relays what peers send; `has_authority` means
  "this peer owns the view", and anti-cheat is the game's problem, exactly as it
  is for the Unity and Unreal SDKs against this same relay.
* **`@rpc` annotations are replaced by `bind_rpc`/`send_rpc`.** Godot resolves
  annotations with its own compiler at export time; a runtime SDK cannot register
  into that table. The hash of the method name on the wire is the same idea, and
  `CYGRpcRouter` is the dispatch table.
* **BaaS speaks `/v2/` REST over `HTTPRequest`.** There is no engine-side
  client to bind against, so `CYGHttp` is pure GDScript and every class in
  `baas/` goes through it.

## Validating a change

The room server must be running (`./target/debug/openoxide-gaming`, health on
`:7777`).

```bash
# 1. everything still compiles
timeout 300 godot --headless --path sdk/godot --import

# 2. two real clients against the relay: 29 checks
timeout 400 godot --headless --path sdk/godot res://test/test_main.tscn
```

`test/test_main.tscn` runs a parent that spawns `bob` detached and `alice` in the
foreground, both against the live relay; the parent retries the pair up to 8
times and prints `PARENT RESULT: n/29 PASS`. Single-role runs are supported for
debugging (`... res://test/test_main.tscn ++ role=alice`).

Alice also drops her own socket at 6.5 s to prove the auto-reconnect path, and
runs the whole session behind `CYGProfilingTransport` so the profiler's byte
totals are counted from real traffic rather than fed.

## License

Apache 2.0 — see [`LICENSE.md`](LICENSE.md). Copyright (c) 2026 CookYourGame Team.
Third-party notices: [`addons/cookyourgame/LICENSE-NETFOX.md`](addons/cookyourgame/LICENSE-NETFOX.md)
and [`addons/cookyourgame/LICENSE-MIRROR.md`](addons/cookyourgame/LICENSE-MIRROR.md).
