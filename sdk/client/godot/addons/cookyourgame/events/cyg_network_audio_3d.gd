class_name CYGNetworkAudio3D
extends CYGBehaviour
## One-shot positional sound, played by every peer from a pre-built pool of
## `AudioStreamPlayer3D` children: firing an event allocates no node.
##
## The Rust relay only forwards RPCs (opcode 6) and view deltas, so an event
## rides `send_rpc(..., Target.OTHERS)`: the relay multicasts the frame to every
## other peer and excludes the sender, which is why the local hear is done
## explicitly by `play_locally` rather than by the router's echo. The world
## position travels on the wire because a proxy's copy of the object is not
## necessarily at the place the sound happened (a muzzle flash, an impact point).
##
## Wire payload, byte for byte, as `CYGRpcRouter.invoke` frames it:
## `[viewId u32 BE][fnv1a("cyg_na3d") u16 BE][target u8 = 1 (OTHERS)]` then the
## router's tagged argument stream, one tag byte per value:
## `[byte 2][LEB128 zigzag identity]` identity packs the dedup key (see below)
## `[byte 6][x f32 BE][y f32 BE][z f32 BE]` world position, 12 bytes
## `[byte 3][volume_db f32 BE]`
## `[byte 3][pitch_scale f32 BE]`
## `identity` is `event_id | sequence << 8 | actor << 16 | tick << 30`, so it is
## the (event id, tick, sender) key plus a per-sender counter that keeps two
## sounds fired in the same tick distinct; it decodes to at most 50 bits and
## costs 6-8 varint bytes. Nothing but that number is needed to replay the event.
##
## Pure one-shot: this component registers no sync property and never calls
## `register_as_observable()`, so it holds no slot in the view's dirty mask and
## `serialize`/`deserialize` stay at the base (no properties, no bytes). Late
## joiners get nothing, which is correct for a footstep — what they need instead
## is `stream`/`streams`/`volume_db`/`pitch_scale`, and those are authored config
## that must match on every peer the way a prefab id does, since only the event
## index travels.
##
## Design derives from Netfox's MIT-licensed event layer (attribution in
## LICENSE-NETFOX.md). Independent GDScript; no Netfox source text is copied.

const RPC_NAME := "cyg_na3d"

const EVENT_ID_BITS := 8
const EVENT_ID_MASK := (1 << EVENT_ID_BITS) - 1
const SEQUENCE_BITS := 8
const SEQUENCE_SHIFT := EVENT_ID_BITS
const SEQUENCE_MASK := (1 << SEQUENCE_BITS) - 1
const ACTOR_BITS := 14
const ACTOR_SHIFT := SEQUENCE_SHIFT + SEQUENCE_BITS
const ACTOR_MASK := (1 << ACTOR_BITS) - 1
const TICK_BITS := 20
const TICK_SHIFT := ACTOR_SHIFT + ACTOR_BITS
const TICK_MASK := (1 << TICK_BITS) - 1

## Power of two: the dedup ring wraps with a mask instead of a modulo.
const DEDUP_RING := 32
const DEDUP_MASK := DEDUP_RING - 1
const MIN_POOL := 1
const MAX_POOL := 16

## Startup-only: a prefab's view id is assigned after `_ready`, and an
## editor-placed view may ready after this node, so binding can legitimately miss
## the first frame.
const BIND_RETRY_LIMIT := 4

## Sound for event id 0 and the fallback for any id when `streams` is empty.
@export var stream: AudioStream = null
## Index = event id, resolved identically on every peer: only the index travels.
@export var streams: Array[AudioStream] = []
## Number of players built once in `_ready`; the whole memory cost of the class.
@export_range(MIN_POOL, MAX_POOL) var pool_size: int = 4
## The sender hears its own event. `Target.OTHERS` never echoes, so this is the
## SDK-side equivalent of Unity's `[SyncMode]` local call.
@export var play_locally: bool = true
## Peers farther than this from the event position skip playback outright.
## 0 disables the check and leaves attenuation to the engine.
@export var max_distance: float = 0.0
## Base loudness every event is measured against.
@export var volume_db: float = 0.0
## Base rate; a per-event pitch multiplies it.
@export var pitch_scale: float = 1.0

var _players: Array[AudioStreamPlayer3D] = []
var _slot_started: Array[int] = []
var _cursor := 0
var _sequence := 0
var _seen: Array[int] = []
var _seen_cursor := 0
var _bound := false
var _bind_attempts := 0
var _warned_silent := false


func _ready() -> void:
	_build_pool()
	_try_bind()


## Called by `CYGView.invoke_spawn_hooks()` once the view has its final id.
func cyg_on_spawned() -> void:
	_seen.fill(0)
	_seen_cursor = 0
	_try_bind()


## ----------------------------------------------------------------- public API

## Fires the sound at this object's own world position.
func play_network_audio(event_id: int = 0, event_volume_db: float = 0.0, event_pitch_scale: float = 1.0) -> bool:
	return play_at(_own_world_position(), event_id, event_volume_db, event_pitch_scale)


## Fires the sound at an explicit world position: hits, impacts and explosions
## happen away from the node that owns them.
func play_at(event_position: Vector3, event_id: int = 0, event_volume_db: float = 0.0, event_pitch_scale: float = 1.0) -> bool:
	var target_view := view
	if target_view == null or target_view.view_id <= 0:
		# No network identity (offline, or the view is still being created): stay
		# audible locally so gameplay code needs no special case.
		_play_local(event_id, event_position, event_volume_db, event_pitch_scale)
		return false
	if not has_authority:
		push_warning("CYGNetworkAudio3D %s: a proxy cannot fire this event; only the view's owner has authority." % name)
		return false
	if play_locally:
		# The relay excludes the sender from an OTHERS multicast and the router
		# echoes no local call for it, so this is the only way the sender hears it.
		_play_local(event_id, event_position, event_volume_db, event_pitch_scale)
	return target_view.send_rpc(
		RPC_NAME,
		[_make_identity(event_id), event_position, event_volume_db, event_pitch_scale],
		CYGOpcodes.Target.OTHERS,
	)


## Stops every live slot; pending RPCs still play when they arrive.
func stop_network_audio() -> void:
	for player in _players:
		if player.playing:
			player.stop()


func is_playing_network() -> bool:
	for player in _players:
		if player.playing:
			return true
	return false


## Re-levels the base loudness for future events and for the slots already play.
func set_volume_db(value: float) -> void:
	volume_db = value
	for player in _players:
		player.volume_db = value


## ------------------------------------------------------------------- internals

func _build_pool() -> void:
	var count := clampi(pool_size, MIN_POOL, MAX_POOL)
	for i in range(count):
		var player := AudioStreamPlayer3D.new()
		player.name = "CYGAudio3DSlot%d" % i
		player.volume_db = volume_db
		player.pitch_scale = pitch_scale
		if max_distance > 0.0:
			# Engine-side cutoff for the pooled player; the script-side check in
			# `_within_listener_range` skips the `play()` call entirely.
			player.max_distance = max_distance
			player.attenuation_model = AudioStreamPlayer3D.ATTENUATION_LOGARITHMIC
		add_child(player)
		_players.append(player)
		_slot_started.append(0)
	_seen.resize(DEDUP_RING)


func _try_bind() -> bool:
	if _bound:
		return true
	var target_view := view
	if target_view == null or target_view.view_id <= 0:
		if _bind_attempts < BIND_RETRY_LIMIT:
			_bind_attempts += 1
			call_deferred("_try_bind")
		return false
	target_view.bind_rpc(RPC_NAME, _on_network_event)
	_bound = true
	return true


## Single argument: the decoded RPC list, per `CYGRpcRouter._execute`.
func _on_network_event(args: Array) -> void:
	if args.size() != 4 or typeof(args[0]) != TYPE_INT or typeof(args[1]) != TYPE_VECTOR3:
		return
	var identity: int = int(args[0])
	if _is_duplicate(identity):
		return
	var event_position: Vector3 = args[1]
	if not _within_listener_range(event_position):
		return
	_play_local(identity & EVENT_ID_MASK, event_position, float(args[2]), float(args[3]))


func _play_local(event_id: int, event_position: Vector3, event_volume_db: float, event_pitch_scale: float) -> void:
	if _players.is_empty():
		return
	var clip := _stream_for(event_id)
	if clip == null:
		return
	var slot := _acquire_slot()
	var player := _players[slot]
	player.stream = clip
	player.global_position = event_position
	player.volume_db = volume_db + event_volume_db
	# A zero rate is not a sound: treat it as "use the authored base pitch".
	player.pitch_scale = pitch_scale * (event_pitch_scale if event_pitch_scale > 0.001 else 1.0)
	player.play()
	_slot_started[slot] = Time.get_ticks_msec()


func _stream_for(event_id: int) -> AudioStream:
	if not streams.is_empty():
		# Masked before the modulo: the remote path always delivers a masked id, and
		# a negative or oversized one from game code must resolve the same way
		# instead of indexing off the end of the array.
		return streams[(event_id & EVENT_ID_MASK) % streams.size()]
	if stream == null and not _warned_silent:
		_warned_silent = true
		push_warning("CYGNetworkAudio3D %s: no stream authored, events are silent." % name)
	return stream


## Round-robin over the pool, preferring an idle player. WHY the fallback: when
## every slot is busy the oldest has already played most of its length, so
## clipping its tail is far less noticeable than losing the event that just
## arrived, and a capped pool is exactly what keeps this class allocation-free.
func _acquire_slot() -> int:
	var count := _players.size()
	for offset in range(count):
		var candidate := _cursor + offset
		if candidate >= count:
			candidate -= count
		if not _players[candidate].playing:
			_cursor = candidate + 1
			if _cursor >= count:
				_cursor = 0
			return candidate
	var oldest := 0
	for i in range(count):
		if _slot_started[i] < _slot_started[oldest]:
			oldest = i
	return oldest


func _make_identity(event_id: int) -> int:
	_sequence = (_sequence + 1) & SEQUENCE_MASK
	return (event_id & EVENT_ID_MASK) \
		| (_sequence << SEQUENCE_SHIFT) \
		| ((CYGNetwork.local_actor_number & ACTOR_MASK) << ACTOR_SHIFT) \
		| ((CYGNetwork.current_tick & TICK_MASK) << TICK_SHIFT)


## Ring of the last identities seen. WHY a ring and not a Dictionary: the window
## only has to outlive a retransmit, and a fixed `Array[int]` neither hashes nor
## allocates on the hot path; scanning 32 integers is cheaper than either.
func _is_duplicate(identity: int) -> bool:
	for i in range(_seen.size()):
		if _seen[i] == identity:
			return true
	_seen[_seen_cursor] = identity
	_seen_cursor = (_seen_cursor + 1) & DEDUP_MASK
	return false


func _within_listener_range(event_position: Vector3) -> bool:
	if max_distance <= 0.0:
		return true
	var viewport := get_viewport()
	if viewport == null:
		return true
	var camera := viewport.get_camera_3d()
	if camera == null:
		# Headless runs and camera-less scenes have no listener to measure from;
		# playing beats dropping the event silently.
		return true
	return camera.global_position.distance_squared_to(event_position) <= max_distance * max_distance


func _own_world_position() -> Vector3:
	var node_3d := get_parent() as Node3D
	return node_3d.global_position if node_3d != null else Vector3()
