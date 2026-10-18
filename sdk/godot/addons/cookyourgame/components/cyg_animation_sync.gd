class_name CYGAnimationSync
extends CYGBehaviour
## Networked `AnimationPlayer` / `AnimationTree` state.
##
## Syncs the playing animation by its index in `AnimationPlayer.get_animation_list()`
## (sorted and scene-deterministic, so it is the stable cross-peer identity; name
## strings travel only for AnimationTree parameters, which are author-named), plus
## loop intent, a custom speed and a coarse phase sample. The authority samples on
## every `CYGNetwork.network_tick` behind thresholds; proxies `play()` the matching
## animation and re-`seek()` only when the phase error exceeds a tolerance, so the
## correction never fights the local player every tick.
##
## Payload after the base sync-property block (this component registers no sync
## properties, so the block is empty): [byte flags] ALWAYS, then in ascending bit
## order FLAG_ANIMATION [varint index u32][bool loop], FLAG_SPEED [single speed],
## FLAG_PHASE [single phase seconds], FLAG_STOP (no body), FLAG_TREE_FIRE
## [string parameter], FLAG_TREE_BLEND [string parameter][vector2].
##
## `animation_speed` from a sibling `CYGCharacterSync3D` (via `speed_source_path`)
## is consumed read-only: it becomes the sampled speed, so locomotion cycles
## retime from the networked velocity without editing that component.
##
## AnimationTree scope, honest: one-shot parameters fire and BlendSpace
## `blend_position`s sync through `AnimationTree.set("parameters/<node>/active")`
## and `.../blend_position` (both verified on Godot 4.7). The *travelled position*
## of an `AnimationStateMachine` node is not synced — there is no public playhead
## API to sample or restore it, and faking one would be speculative; games that
## need it drive the machine from the synced parameters like Netfox-style SDKs do.
##
## All state rides STATE_DELTA (op 5) through the view's dirty mask; the Rust relay
## drops op 4/7, so no other channel is used.

const FLAG_ANIMATION := 1
const FLAG_SPEED := 2
const FLAG_PHASE := 4
const FLAG_STOP := 8
const FLAG_TREE_FIRE := 16
const FLAG_TREE_BLEND := 32

## Sibling `AnimationPlayer` to sample and replay.
@export var animation_player_path: NodePath = ^"../AnimationPlayer"
## Optional sibling `AnimationTree`; when set, tree events sync too.
@export var animation_tree_path: NodePath = NodePath()
## Optional node exposing `animation_speed` (a `CYGCharacterSync3D`); when it
## resolves, its value replaces `get_playing_speed()` as the sampled speed.
@export var speed_source_path: NodePath = NodePath()
## Proxy re-seeks when the phase error exceeds this many seconds.
@export var phase_tolerance: float = 0.12
## Authority sends a speed update when it moves more than this (multiplier units).
@export var speed_threshold: float = 0.05

var _player: AnimationPlayer = null
var _tree: AnimationTree = null
var _speed_source: Node = null

# Identity table: index -> animation name, resolved from the LOCAL player so each
# peer maps the shared index through its own (scene-identical) list.
var _names: PackedStringArray = PackedStringArray()

var _state_dirty := false
var _out_flags := 0
var _out_index := 0
var _out_loop := false
var _out_speed := 1.0
var _out_phase := 0.0
var _out_param := ""
var _out_blend := Vector2()

# Authority-side shadow of what the proxies should be holding, so corrections are
# sent on measured divergence instead of every tick. No engine playhead getter is
# trusted here: Godot 4.7 exposes no animation-time read on AnimationPlayer.
var _sent_index := -1
var _sent_speed := 1.0
var _sent_phase := 0.0
var _ticks_since_sent := 0
var _shadow_phase := 0.0
var _was_playing := false

# Proxy-side mirror of the expected playhead, used to preserve phase across a
# speed-only re-play (AnimationPlayer has no in-place custom-speed update).
var _proxy_speed := 1.0
var _proxy_phase := 0.0
var _proxy_tracking := false

var _ticked := false
var _warned_unknown_anim := false


func _ready() -> void:
	_player = get_node_or_null(animation_player_path) as AnimationPlayer
	if _player == null:
		push_warning("CYGAnimationSync on %s: no AnimationPlayer at '%s'; animation state will not sync."
			% [name, animation_player_path])
	_tree = get_node_or_null(animation_tree_path) as AnimationTree
	if speed_source_path != NodePath():
		_speed_source = get_node_or_null(speed_source_path)
	_refresh_names()
	# Observable registration is unconditional and identical on every peer.
	register_as_observable()
	CYGNetwork.network_tick.connect(_on_network_tick)
	_ticked = true


func _exit_tree() -> void:
	if _ticked:
		CYGNetwork.network_tick.disconnect(_on_network_tick)
		_ticked = false
	super._exit_tree()


func cyg_on_spawned() -> void:
	_sent_index = -1
	_sent_speed = 1.0
	_sent_phase = 0.0
	_ticks_since_sent = 0
	_shadow_phase = 0.0
	_was_playing = false
	_proxy_tracking = false
	_state_dirty = false
	_out_flags = 0
	_refresh_names()


## Resolves the current animation name through this peer's list (debug use).
func animation_name() -> StringName:
	return _player.get_current_animation() if _player != null else &""


## True on proxies while the last synced animation loops (the loop bit is intent:
## the engine's loop mode comes from the shared Animation resource).
var remote_animation_loops: bool:
	get:
		return _out_loop


## Authority convenience: play locally; the tick sampler syncs the change.
func play(animation_name: StringName, custom_speed: float = 1.0) -> bool:
	if _player == null or not _player.has_animation(animation_name):
		return false
	_player.play(animation_name, -1.0, custom_speed)
	return true


## Fires an `AnimationNodeOneShot` parameter locally and on every proxy.
func fire_one_shot(parameter: String) -> void:
	if not has_authority:
		push_warning("CYGAnimationSync.fire_one_shot('%s') ignored: no authority." % parameter)
		return
	if _tree != null:
		_tree.set("parameters/%s/active" % parameter, true)
	_out_param = parameter
	_out_flags |= FLAG_TREE_FIRE
	_state_dirty = true


## Sets a BlendSpace `blend_position` locally and on every proxy.
func set_blend_position(parameter: String, position: Vector2) -> void:
	if not has_authority:
		push_warning("CYGAnimationSync.set_blend_position('%s') ignored: no authority." % parameter)
		return
	if _tree != null:
		_tree.set("parameters/%s/blend_position" % parameter, position)
	_out_param = parameter
	_out_blend = position
	_out_flags |= FLAG_TREE_BLEND
	_state_dirty = true


func has_delta() -> bool:
	return _state_dirty or super.has_delta()


func serialize(writer: CYGBitPacker) -> void:
	super.serialize(writer)
	# The flags byte is written even when zero so deserialize's fixed layout stays
	# symmetric with property-only deltas.
	writer.write_byte(_out_flags)
	if not _state_dirty:
		return

	if (_out_flags & FLAG_ANIMATION) != 0:
		writer.write_uint32(_out_index)
		writer.write_bool(_out_loop)
	if (_out_flags & FLAG_SPEED) != 0:
		writer.write_single(_out_speed)
	if (_out_flags & FLAG_PHASE) != 0:
		writer.write_single(_out_phase)
	if (_out_flags & FLAG_TREE_FIRE) != 0:
		writer.write_string(_out_param)
	if (_out_flags & FLAG_TREE_BLEND) != 0:
		writer.write_string(_out_param)
		writer.write_vector2(_out_blend)
	_out_flags = 0
	_state_dirty = false


func deserialize(reader: CYGBitReader) -> void:
	super.deserialize(reader)

	var flags := reader.read_byte()
	# Consume exactly what serialize wrote, authority or not, so the shared
	# reader stays aligned for the components behind this one.
	var index := _sent_index
	var loop := _out_loop
	var speed := _proxy_speed
	var has_phase := false
	var phase := 0.0
	var has_animation := (flags & FLAG_ANIMATION) != 0
	var has_stop := (flags & FLAG_STOP) != 0
	var fire_param := ""
	var blend_param := ""
	var blend_position := Vector2()
	if has_animation:
		index = int(reader.read_uint32())
		loop = reader.read_bool()
	if (flags & FLAG_SPEED) != 0:
		speed = reader.read_single()
	if (flags & FLAG_PHASE) != 0:
		phase = reader.read_single()
		has_phase = true
	if (flags & FLAG_TREE_FIRE) != 0:
		fire_param = reader.read_string()
	if (flags & FLAG_TREE_BLEND) != 0:
		blend_param = reader.read_string()
		blend_position = reader.read_vector2()

	if has_authority:
		return

	if _tree != null and not fire_param.is_empty():
		_tree.set("parameters/%s/active" % fire_param, true)
	if _tree != null and not blend_param.is_empty():
		_tree.set("parameters/%s/blend_position" % blend_param, blend_position)
	if _player == null:
		return

	if has_stop:
		_player.stop()
		_proxy_tracking = false
	if has_phase:
		_proxy_phase = phase
	if (flags & FLAG_SPEED) != 0:
		_proxy_speed = speed

	if has_animation:
		_out_loop = loop
		if index >= 0 and index < _names.size():
			var resolved := _names[index]
			if _player.has_animation(resolved):
				_player.play(resolved, -1.0, _proxy_speed)
				if has_phase:
					_player.seek(_proxy_phase)
				_proxy_tracking = true
		elif not _warned_unknown_anim:
			_warned_unknown_anim = true
			push_warning("CYGAnimationSync: index %d is not in this peer's animation list; scene mismatch?" % index)
	elif (flags & FLAG_SPEED) != 0 and _player.is_playing():
		# No in-place custom-speed API exists, so re-play holding the tracked phase.
		_player.play(_player.get_current_animation(), -1.0, _proxy_speed)
		if _proxy_tracking:
			_player.seek(_proxy_phase)
	elif has_phase and _proxy_tracking:
		_player.seek(_proxy_phase)


## Proxy-side shadow: keeps the expected playhead so a speed-only update can
## re-play without a visible jump.
func _process(delta: float) -> void:
	if _proxy_tracking and _player != null and _player.is_playing():
		_proxy_phase += _proxy_speed * delta


func _on_network_tick(_tick: int) -> void:
	if not has_authority or _player == null:
		return
	var tick_seconds := 1.0 / float(maxi(CYGNetwork.tick_rate, 1))
	var speed := _sample_speed()
	var flags := 0

	if _player.is_playing():
		var current := _player.get_current_animation()
		var index := _names.find(current)
		if index < 0:
			# An animation assigned after scene load (code-added library): refresh
			# once before giving up on the frame.
			_refresh_names()
			index = _names.find(current)
		if index >= 0 and index != _sent_index:
			_sent_index = index
			_out_index = index
			_out_loop = _animation_loops(current)
			_shadow_phase = 0.0
			flags |= FLAG_ANIMATION | FLAG_PHASE
		elif index >= 0:
			_shadow_phase += speed * tick_seconds
		if absf(speed - _sent_speed) > speed_threshold:
			flags |= FLAG_SPEED
		# WHY divergence-tested instead of sent per tick: the proxies advance at the
		# same rate; only a dropped packet or a seek can open a gap worth bytes.
		var expected_proxy := _sent_phase + _sent_speed * float(_ticks_since_sent) * tick_seconds
		if absf(_shadow_phase - expected_proxy) > phase_tolerance:
			flags |= FLAG_PHASE
		_was_playing = true
	elif _was_playing:
		_was_playing = false
		_sent_index = -1
		_proxy_offline_stop()
		flags |= FLAG_STOP

	if flags == 0:
		_ticks_since_sent += 1
		return

	if (flags & FLAG_SPEED) != 0:
		_sent_speed = speed
		_out_speed = speed
	if (flags & FLAG_PHASE) != 0:
		_sent_phase = _shadow_phase
		_out_phase = _shadow_phase
	_out_flags |= flags
	_state_dirty = true
	_ticks_since_sent = 0


## The sampled speed: the networked character's `animation_speed` when a source
## resolved, otherwise the player's own playing speed.
func _sample_speed() -> float:
	if _speed_source != null and is_instance_valid(_speed_source):
		var value: Variant = _speed_source.get("animation_speed")
		if typeof(value) == TYPE_FLOAT or typeof(value) == TYPE_INT:
			return float(value)
	return _player.get_playing_speed()


func _animation_loops(animation_name: StringName) -> bool:
	var animation := _player.get_animation(animation_name)
	return animation != null and animation.loop_mode != Animation.LOOP_NONE


func _proxy_offline_stop() -> void:
	_sent_phase = 0.0
	_sent_speed = _sample_speed()
	_shadow_phase = 0.0


func _refresh_names() -> void:
	if _player != null:
		_names = _player.get_animation_list()
