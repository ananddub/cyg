class_name CYGNetworkAudio2D
extends CYGBehaviour
## One-shot non-positional sound (music stabs, UI, 2D cues), played by every peer
## from a pre-built pool of `AudioStreamPlayer2D` children.
##
## Position is deliberately absent: an `AudioStreamPlayer2D` mixes at the
## listener, so the event carries only what changes between cues. Like the 3D
## variant it rides `send_rpc(..., Target.OTHERS)` because the relay forwards no
## opcode but RPCs and view deltas, and the relay excludes the sender from that
## multicast — hence `play_locally` does the sender's own hear.
##
## Wire payload, byte for byte, as `CYGRpcRouter.invoke` frames it:
## `[viewId u32 BE][fnv1a("cyg_na2d") u16 BE][target u8 = 1 (OTHERS)]` then the
## router's tagged argument stream:
## `[byte 2][LEB128 zigzag identity]` identity = `event_id | sequence << 8 |
##   actor << 16 | tick << 30` — the (event id, tick, sender) dedup key with a
##   per-sender counter so two cues in one tick stay distinct; 6-8 varint bytes
## `[byte 3][volume_db f32 BE]`
## `[byte 3][pitch_scale f32 BE]`
##
## Pure one-shot: no sync property, no `register_as_observable()`, so this
## component never occupies a dirty-mask bit and inherits the base
## `serialize`/`deserialize`, which write and read nothing because nothing was
## registered. A late joiner cannot hear a cue that already fired; it only needs
## `stream`/`streams`/`volume_db`/`pitch_scale` to be authored identically on
## every peer, because only the event index travels.
##
## Design derives from Netfox's MIT-licensed event layer (attribution in
## LICENSE-NETFOX.md). Clean-room GDScript; no Netfox source text is copied.

const RPC_NAME := "cyg_na2d"

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
const BIND_RETRY_LIMIT := 4

## Sound for event id 0 and the fallback for any id when `streams` is empty.
@export var stream: AudioStream = null
## Index = event id, resolved identically on every peer: only the index travels.
@export var streams: Array[AudioStream] = []
## Players built once in `_ready`; the whole memory cost of the class.
@export_range(MIN_POOL, MAX_POOL) var pool_size: int = 4
## The sender hears its own event; `Target.OTHERS` never echoes.
@export var play_locally: bool = true
@export var volume_db: float = 0.0
@export var pitch_scale: float = 1.0

var _players: Array[AudioStreamPlayer2D] = []
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

## Fires the cue on every peer.
func play_network_audio(event_id: int = 0, event_volume_db: float = 0.0, event_pitch_scale: float = 1.0) -> bool:
	var target_view := view
	if target_view == null or target_view.view_id <= 0:
		# No network identity (offline, or the view is still being created): stay
		# audible locally so gameplay code needs no special case.
		_play_local(event_id, event_volume_db, event_pitch_scale)
		return false
	if not has_authority:
		push_warning("CYGNetworkAudio2D %s: a proxy cannot fire this event; only the view's owner has authority." % name)
		return false
	if play_locally:
		_play_local(event_id, event_volume_db, event_pitch_scale)
	return target_view.send_rpc(
		RPC_NAME,
		[_make_identity(event_id), event_volume_db, event_pitch_scale],
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
		var player := AudioStreamPlayer2D.new()
		player.name = "CYGAudio2DSlot%d" % i
		player.volume_db = volume_db
		player.pitch_scale = pitch_scale
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
	if args.size() != 3 or typeof(args[0]) != TYPE_INT:
		return
	var identity: int = int(args[0])
	if _is_duplicate(identity):
		return
	_play_local(identity & EVENT_ID_MASK, float(args[1]), float(args[2]))


func _play_local(event_id: int, event_volume_db: float, event_pitch_scale: float) -> void:
	if _players.is_empty():
		return
	var clip := _stream_for(event_id)
	if clip == null:
		return
	var slot := _acquire_slot()
	var player := _players[slot]
	player.stream = clip
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
		push_warning("CYGNetworkAudio2D %s: no stream authored, events are silent." % name)
	return stream


## Round-robin over the pool, preferring an idle player. WHY the fallback: when
## every slot is busy the oldest has already played most of its length, so
## clipping its tail is far less noticeable than losing the cue that just
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
## allocates on the hot path.
func _is_duplicate(identity: int) -> bool:
	for i in range(_seen.size()):
		if _seen[i] == identity:
			return true
	_seen[_seen_cursor] = identity
	_seen_cursor = (_seen_cursor + 1) & DEDUP_MASK
	return false
