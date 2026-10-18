class_name CYGNetworkParticles2D
extends CYGBehaviour
## One-shot 2D particle burst restarted on a pre-built pool of emitters, so a hit
## costs no `instantiate()` and no node spawn at event time.
##
## Rides `send_rpc(..., Target.OTHERS)` like the other events — the relay only
## forwards RPCs and view deltas, and excludes the sender from that multicast, so
## `play_locally` is what shows the burst on the peer that fired it. The world
## position travels explicitly because a proxy's copy of the shooter is not at
## the impact point.
##
## Wire payload, byte for byte, as `CYGRpcRouter.invoke` frames it:
## `[viewId u32 BE][fnv1a("cyg_np2d") u16 BE][target u8 = 1 (OTHERS)]` then:
## `[byte 2][LEB128 zigzag identity]` identity = `event_id | sequence << 8 |
##   actor << 16 | tick << 30` — the (event id, tick, sender) dedup key plus a
##   per-sender counter so two bursts in one tick stay distinct; 6-8 varint bytes
## `[byte 5][x f32 BE][y f32 BE]` canvas-space position, 8 bytes
## `[byte 3][rotation f32 BE]` emitter roll in radians; 0 keeps the authored facing
## `[byte 3][size_scale f32 BE]` multiplier on the effect, so one class covers a
##   spark and a fireball
##
## Pure one-shot: no sync property, no `register_as_observable()`, so this
## component never occupies a dirty-mask bit and inherits the base
## `serialize`/`deserialize`, which write and read nothing because nothing was
## registered. A late joiner never sees a burst that already finished; what it
## must author identically is `effect_scene`/`effect_scenes`, since only the
## effect index travels.
##
## Design derives from Netfox's MIT-licensed event layer (attribution in
## LICENSE-NETFOX.md). Clean-room GDScript; no Netfox source text is copied.

const RPC_NAME := "cyg_np2d"

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

const DEDUP_RING := 32
const DEDUP_MASK := DEDUP_RING - 1
const MIN_POOL := 1
const MAX_POOL := 16
const BIND_RETRY_LIMIT := 4
## A slot is released on the emitter's `finished` signal; this margin only bounds
## the wait for an effect that never reports one.
const BUSY_LIFETIME_MARGIN := 1.25
## Side of the shared white pixel the zero-asset fallback draws with.
const FALLBACK_TEXTURE_SIZE := 4

## Effect for event id 0 and the fallback when `effect_scenes` is empty; a null
## here builds the parameterless CPU burst described on `_build_fallback_slot()`.
@export var effect_scene: PackedScene = null
## Index = event id, resolved identically on every peer: only the index travels.
@export var effect_scenes: Array[PackedScene] = []
## Emitters per registered effect, built once in `_ready`; the pool holds
## `pool_size * max(1, effect_scenes.size())` effects and never grows.
@export_range(MIN_POOL, MAX_POOL) var pool_size: int = 4
## The firing peer shows the burst too; `Target.OTHERS` never echoes.
@export var play_locally: bool = true
## Busy window for an emitter that reports no lifetime.
@export var fallback_lifetime: float = 1.0

var _containers: Array[Node2D] = []
var _emitters: Array[Node] = []
var _slot_event: Array[int] = []
var _slot_started: Array[int] = []
var _slot_free_at: Array[int] = []
var _cursor := 0
var _sequence := 0
var _seen: Array[int] = []
var _seen_cursor := 0
var _bound := false
var _bind_attempts := 0
# One shared texture for every synthesized slot: the fallback must not cost an
# image and a texture per emitter.
var _fallback_texture: ImageTexture = null


func _ready() -> void:
	_build_pool()
	_try_bind()


## Called by `CYGView.invoke_spawn_hooks()` once the view has its final id.
func cyg_on_spawned() -> void:
	_seen.fill(0)
	_seen_cursor = 0
	_try_bind()


## ----------------------------------------------------------------- public API

## Bursts at this object's own position, upright.
func emit_network_particles(event_id: int = 0, size_scale: float = 1.0) -> bool:
	var node_2d := get_parent() as Node2D
	if node_2d == null:
		return emit_at(Vector2(), 0.0, event_id, size_scale)
	return emit_at(node_2d.global_position, node_2d.global_rotation, event_id, size_scale)


## Bursts at an explicit canvas position, rolled to `rotation_radians` (0 keeps
## the authored facing).
func emit_at(world_position: Vector2, rotation_radians: float, event_id: int = 0, size_scale: float = 1.0) -> bool:
	var target_view := view
	if target_view == null or target_view.view_id <= 0:
		# No network identity (offline, or the view is still being created): show it
		# locally anyway so gameplay code needs no special case.
		_emit_local(event_id, world_position, rotation_radians, size_scale)
		return false
	if not has_authority:
		push_warning("CYGNetworkParticles2D %s: a proxy cannot fire this event; only the view's owner has authority." % name)
		return false
	if play_locally:
		_emit_local(event_id, world_position, rotation_radians, size_scale)
	return target_view.send_rpc(
		RPC_NAME,
		[_make_identity(event_id), world_position, rotation_radians, size_scale],
		CYGOpcodes.Target.OTHERS,
	)


## Hides every live burst and frees its slot.
func stop_network_particles() -> void:
	for i in range(_emitters.size()):
		_set_emitting(_emitters[i], false)
		_slot_free_at[i] = 0
	_cursor = 0


func is_emitting_network() -> bool:
	var now := Time.get_ticks_msec()
	for i in range(_emitters.size()):
		if _slot_free_at[i] > now and _is_emitting(_emitters[i]):
			return true
	return false


## ------------------------------------------------------------------- internals

func _build_pool() -> void:
	var slots_per_effect := clampi(pool_size, MIN_POOL, MAX_POOL)
	var effect_count := maxi(effect_scenes.size(), 1)
	for event_id in range(effect_count):
		var scene := _scene_for(event_id)
		for slot in range(slots_per_effect):
			var container := Node2D.new()
			container.name = "CYGBurst2D_e%d_s%d" % [event_id, slot]
			add_child(container)
			var emitter := _make_emitter(scene, container)
			_configure_one_shot(emitter)
			if emitter.has_signal("finished"):
				# `bind()` bakes the slot index once here, so the handler needs no
				# per-event allocation and no closure over a loop variable.
				emitter.connect("finished", _on_slot_finished.bind(_emitters.size()))
			_containers.append(container)
			_emitters.append(emitter)
			_slot_event.append(event_id & EVENT_ID_MASK)
			_slot_started.append(0)
			# 0 means "free"; the deadline is written when the burst fires.
			_slot_free_at.append(0)
	_seen.resize(DEDUP_RING)


## Instantiates the authored effect once per slot, or synthesizes a CPU burst.
func _make_emitter(scene: PackedScene, container: Node2D) -> Node:
	if scene == null:
		var burst := _build_fallback_slot()
		container.add_child(burst)
		return burst
	var instance := scene.instantiate()
	container.add_child(instance)
	var found := _find_particles(instance)
	if found != null:
		return found
	push_warning("CYGNetworkParticles2D %s: '%s' has no particle node; the instance root is pooled instead." % [name, scene.resource_path])
	return instance


## Zero-asset fallback: a short radial CPU burst drawn with one shared white
## quad, so an events node dropped into a greybox scene is visible and a headless
## run still has an emitter to exercise.
func _build_fallback_slot() -> CPUParticles2D:
	if _fallback_texture == null:
		var image := Image.create(FALLBACK_TEXTURE_SIZE, FALLBACK_TEXTURE_SIZE, false, Image.FORMAT_RGBA8)
		image.fill(Color(1, 1, 1, 1))
		_fallback_texture = ImageTexture.create_from_image(image)
	var burst := CPUParticles2D.new()
	burst.name = "CYGBurst2D"
	burst.texture = _fallback_texture
	burst.amount = 24
	burst.lifetime = fallback_lifetime
	burst.one_shot = true
	burst.explosiveness = 1.0
	burst.direction = Vector2(0, -1)
	burst.spread = 180.0
	burst.gravity = Vector2.ZERO
	burst.initial_velocity_min = 40.0
	burst.initial_velocity_max = 90.0
	burst.emitting = false
	return burst


func _find_particles(node: Node) -> Node:
	if node is GPUParticles2D or node is CPUParticles2D:
		return node
	for child in node.get_children():
		var found := _find_particles(child)
		if found != null:
			return found
	return null


## Events are one-shot by contract, so an authored looping effect is converted
## once at build time; without this a pooled slot could never be reused.
func _configure_one_shot(emitter: Node) -> void:
	if emitter.get("one_shot") != null:
		emitter.set("one_shot", true)
	if emitter.get("preprocess") != null:
		emitter.set("preprocess", 0.0)
	# 4.7 gives 2D particles no `visibility_aabb`: `GPUParticles2D` culls against
	# `visibility_rect`, and a scene that never set one keeps the default empty rect,
	# which is the classic "my particles never appear" bug. `CPUParticles2D` has no
	# such property at all (verified against 4.7's property list), so a null read
	# means "nothing to fix" and an empty rect is the only one replaced.
	var rect: Variant = emitter.get("visibility_rect")
	if rect == null:
		return
	if rect is Rect2 and (rect as Rect2).has_area():
		return
	emitter.set("visibility_rect", Rect2(-256, -256, 512, 512))


func _scene_for(event_id: int) -> PackedScene:
	if not effect_scenes.is_empty():
		# Modulo keeps a stale or out-of-range id visible rather than invisible.
		return effect_scenes[event_id % effect_scenes.size()]
	return effect_scene


## Single argument: the decoded RPC list, per `CYGRpcRouter._execute`.
func _on_network_event(args: Array) -> void:
	if args.size() != 4 or typeof(args[0]) != TYPE_INT or typeof(args[1]) != TYPE_VECTOR2:
		return
	var identity: int = int(args[0])
	if _is_duplicate(identity):
		return
	_emit_local(identity & EVENT_ID_MASK, args[1], float(args[2]), float(args[3]))


func _emit_local(event_id: int, world_position: Vector2, rotation_radians: float, size_scale: float) -> void:
	if _emitters.is_empty():
		return
	var slot := _acquire_slot(event_id)
	var container := _containers[slot]
	var emitter := _emitters[slot]
	container.global_position = world_position
	container.global_rotation = rotation_radians
	container.scale = Vector2.ONE * (size_scale if size_scale > 0.001 else 1.0)
	_set_emitting(emitter, true)
	if emitter.has_method("restart"):
		emitter.call("restart")
	var now := Time.get_ticks_msec()
	_slot_started[slot] = now
	_slot_free_at[slot] = now + int(_lifetime_of(emitter) * 1000.0 * BUSY_LIFETIME_MARGIN)


func _lifetime_of(emitter: Node) -> float:
	var reported: Variant = emitter.get("lifetime")
	var seconds := fallback_lifetime if reported == null else float(reported)
	return seconds if seconds > 0.001 else fallback_lifetime


func _set_emitting(emitter: Node, value: bool) -> void:
	emitter.set("emitting", value)


func _is_emitting(emitter: Node) -> bool:
	var reported: Variant = emitter.get("emitting")
	return false if reported == null else bool(reported)


## Round-robin among the slots registered for this event id, preferring a burst
## past its deadline and otherwise taking the oldest live one. WHY: the oldest
## burst is closest to finishing, so clipping it beats dropping the hit that
## just landed — and the capped pool is what keeps this class allocation-free.
func _acquire_slot(event_id: int) -> int:
	var count := _emitters.size()
	var wanted := event_id & EVENT_ID_MASK
	var now := Time.get_ticks_msec()
	var oldest := -1
	for offset in range(count):
		var candidate := _cursor + offset
		if candidate >= count:
			candidate -= count
		if _slot_event[candidate] != wanted:
			continue
		if oldest < 0:
			oldest = candidate
		elif _slot_started[candidate] < _slot_started[oldest]:
			oldest = candidate
		if _slot_free_at[candidate] <= now:
			_cursor = candidate + 1
			if _cursor >= count:
				_cursor = 0
			return candidate
	if oldest >= 0:
		return oldest
	# An id with no registered slot falls back to slot 0, which always carries the
	# `_scene_for()` fallback effect.
	return 0


func _on_slot_finished(slot: int) -> void:
	if slot >= 0 and slot < _slot_free_at.size():
		_slot_free_at[slot] = 0


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
