class_name CYGTransformSync2D
extends CYGBehaviour
## Networked 2D Transform. The authority samples the parent Node2D pose on each
## fixed network tick and sends threshold-passed deltas; proxies buffer incoming
## snapshots and interpolate at renderTime = now - 100ms. Authoritative receives
## are decoded (to keep the shared reader aligned) but never write the pose, so
## the local simulation owns it.
##
## Payload after the base sync-property block: [byte flags] always, then in flag
## order position (Vector2 packed not used; single floats) / rotation (one float
## radians) / scale (Vector2). Mirrors CYGTransformSync3D / the Unity reference.
##
## Uses the parent's LOCAL pose (position/rotation/scale) so a scene works
## standalone under any parent.

const BUFFER_CAPACITY := 32
const BUFFER_MASK := BUFFER_CAPACITY - 1
const INTERPOLATION_DELAY := 0.1
const MAX_EXTRAPOLATION := 0.2
const POSITION_THRESHOLD_SQR := 0.01 * 0.01
const ROTATION_THRESHOLD_DEG := 0.05
const SCALE_THRESHOLD_SQR := 1e-6
## WHY: a >5 unit jump almost always means respawn/teleport, not motion;
## interpolating the gap looks like flying across the map, so both sides snap.
const TELEPORT_DISTANCE_SQR := 5.0 * 5.0

const FLAG_POSITION := 1
const FLAG_ROTATION := 2
const FLAG_SCALE := 4
const FLAG_TELEPORT := 8

## Also sync local scale (off by default; usually authored, not simulated).
@export var sync_scale: bool = false

var _node: Node2D = null

# Pre-sized parallel ring arrays: no allocation on the hot path.
var _time_ring: Array[float] = []
var _pos_ring: Array[Vector2] = []
var _rot_ring: Array[float] = []
var _vel_ring: Array[Vector2] = []
var _head := 0
var _count := 0

var _pose_dirty := false
var _out_flags := 0
var _out_position := Vector2()
var _out_rotation := 0.0
var _out_scale := Vector2()

var _last_sent_position := Vector2()
var _last_sent_rotation := 0.0
var _last_sent_scale := Vector2()
var _has_sent_baseline := false

var _last_incoming_position := Vector2()
var _last_incoming_rotation := 0.0
var _last_incoming_time := 0.0
var _has_incoming_baseline := false

var _ticked := false


func _ready() -> void:
	_node = get_parent() as Node2D
	if _node == null:
		push_warning("CYGTransformSync2D on %s: parent is not a Node2D; nothing to sync." % name)
		return
	for _i in range(BUFFER_CAPACITY):
		_time_ring.append(0.0)
		_pos_ring.append(Vector2())
		_rot_ring.append(0.0)
		_vel_ring.append(Vector2())
	register_as_observable()
	CYGNetwork.network_tick.connect(_on_network_tick)
	_ticked = true


func _exit_tree() -> void:
	if _ticked:
		CYGNetwork.network_tick.disconnect(_on_network_tick)
		_ticked = false
	super._exit_tree()


func cyg_on_spawned() -> void:
	_has_sent_baseline = false
	_has_incoming_baseline = false
	_pose_dirty = false
	_out_flags = 0
	_head = 0
	_count = 0


func _on_network_tick(_tick: int) -> void:
	if not has_authority or _node == null:
		return

	var position := _node.position
	var rotation := _node.rotation
	var scale := _node.scale

	if not _has_sent_baseline:
		_last_sent_position = position
		_last_sent_rotation = rotation
		_last_sent_scale = scale
		_has_sent_baseline = true
		return

	var flags := 0
	var position_delta_sqr := (position - _last_sent_position).length_squared()
	if position_delta_sqr >= POSITION_THRESHOLD_SQR:
		flags |= FLAG_POSITION
	if absf(rad_to_deg(wrapf(rotation - _last_sent_rotation, -PI, PI))) > ROTATION_THRESHOLD_DEG:
		flags |= FLAG_ROTATION
	if sync_scale and (scale - _last_sent_scale).length_squared() >= SCALE_THRESHOLD_SQR:
		flags |= FLAG_SCALE
	if flags == 0:
		return
	if position_delta_sqr >= TELEPORT_DISTANCE_SQR:
		flags |= FLAG_TELEPORT

	_out_flags |= flags
	_out_position = position
	_out_rotation = rotation
	_out_scale = scale
	_last_sent_position = position
	_last_sent_rotation = rotation
	_last_sent_scale = scale
	_pose_dirty = true


func has_delta() -> bool:
	return _pose_dirty or super.has_delta()


func serialize(writer: CYGBitPacker) -> void:
	super.serialize(writer)
	# The flags byte is written even when zero so deserialize's fixed layout stays
	# symmetric with sync-var-only deltas.
	writer.write_byte(_out_flags)
	if not _pose_dirty:
		return

	if (_out_flags & FLAG_POSITION) != 0:
		writer.write_vector2(_out_position)
	if (_out_flags & FLAG_ROTATION) != 0:
		writer.write_single(_out_rotation)
	if (_out_flags & FLAG_SCALE) != 0:
		writer.write_vector2(_out_scale)
	_out_flags = 0
	_pose_dirty = false


func deserialize(reader: CYGBitReader) -> void:
	super.deserialize(reader)

	var flags := reader.read_byte()
	var position := _last_incoming_position if _has_incoming_baseline else (_node.position if _node != null else Vector2())
	var rotation := _last_incoming_rotation if _has_incoming_baseline else (_node.rotation if _node != null else 0.0)

	if (flags & FLAG_POSITION) != 0:
		position = reader.read_vector2()
	if (flags & FLAG_ROTATION) != 0:
		rotation = reader.read_single()
	var has_scale := (flags & FLAG_SCALE) != 0
	var scale := Vector2()
	if has_scale:
		scale = reader.read_vector2()

	# Authority decoded everything above to keep the reader aligned, but must not
	# move the node: the local simulation owns the pose.
	if has_authority:
		return

	if has_scale and _node != null:
		_node.scale = scale

	var now: float = Time.get_ticks_usec() / 1000000.0
	var velocity := Vector2()
	if _has_incoming_baseline:
		var delta_time: float = now - _last_incoming_time
		if delta_time > 0.000001 and delta_time < 1.0:
			velocity = (position - _last_incoming_position) / delta_time
	_last_incoming_position = position
	_last_incoming_rotation = rotation
	_last_incoming_time = now
	_has_incoming_baseline = true

	var teleport := (flags & FLAG_TELEPORT) != 0
	if not teleport and _count > 0:
		teleport = (position - _pos_ring[_ring_index(_count - 1)]).length_squared() >= TELEPORT_DISTANCE_SQR
	if teleport or _count == 0:
		_head = 0
		_count = 0
		_push_snapshot(position, rotation, Vector2(), now)
		if _node != null:
			_node.position = position
			_node.rotation = rotation
		return

	_push_snapshot(position, rotation, velocity, now)


func _push_snapshot(position: Vector2, rotation: float, velocity: Vector2, now: float) -> void:
	_time_ring[_head] = now
	_pos_ring[_head] = position
	_rot_ring[_head] = rotation
	_vel_ring[_head] = velocity
	_head = (_head + 1) & BUFFER_MASK
	if _count < BUFFER_CAPACITY:
		_count += 1


func _ring_index(index_from_oldest: int) -> int:
	return (_head - _count + index_from_oldest) & BUFFER_MASK


func _process(_delta: float) -> void:
	if _node == null or _count == 0 or has_authority:
		return

	var now: float = Time.get_ticks_usec() / 1000000.0
	var render_time: float = now - INTERPOLATION_DELAY
	var newest := _ring_index(_count - 1)
	var oldest := _ring_index(0)

	if render_time >= _time_ring[newest]:
		var extrap: float = render_time - _time_ring[newest]
		if extrap > MAX_EXTRAPOLATION:
			extrap = MAX_EXTRAPOLATION
		_node.position = _pos_ring[newest] + _vel_ring[newest] * extrap
		_node.rotation = _rot_ring[newest]
		return

	if render_time <= _time_ring[oldest]:
		_node.position = _pos_ring[oldest]
		_node.rotation = _rot_ring[oldest]
		return

	for i in range(_count - 1):
		var a := _ring_index(i)
		var b := _ring_index(i + 1)
		if render_time < _time_ring[a] or render_time > _time_ring[b]:
			continue

		var span: float = _time_ring[b] - _time_ring[a]
		if span <= 0.000001:
			_node.position = _pos_ring[b]
			_node.rotation = _rot_ring[b]
			return

		var t: float = (render_time - _time_ring[a]) / span
		var dt: float = span
		# WHY: cubic Hermite through both snapshots with each side's velocity as its
		# tangent keeps C1 continuity across joins, so the proxy never kinks.
		var t2: float = t * t
		var t3: float = t2 * t
		var interpolated := (2.0 * t3 - 3.0 * t2 + 1.0) * _pos_ring[a] \
			+ (t3 - 2.0 * t2 + t) * _vel_ring[a] * dt \
			+ (-2.0 * t3 + 3.0 * t2) * _pos_ring[b] \
			+ (t3 - t2) * _vel_ring[b] * dt

		_node.position = interpolated
		_node.rotation = lerp_angle(_rot_ring[a], _rot_ring[b], t)
		return


## Teleports the pose and broadcasts the teleport flag so proxies skip interpolation.
func snap_position(value: Vector2) -> void:
	if _node == null:
		return
	_node.position = value
	_last_sent_position = value

	if has_authority:
		_out_position = value
		_out_rotation = _node.rotation
		_out_scale = _node.scale
		_out_flags |= FLAG_POSITION | FLAG_TELEPORT
		_pose_dirty = true
	else:
		_head = 0
		_count = 0
		var now: float = Time.get_ticks_usec() / 1000000.0
		_push_snapshot(value, _node.rotation, Vector2(), now)
		_last_incoming_position = value
		_last_incoming_rotation = _node.rotation
		_last_incoming_time = now
		_has_incoming_baseline = true


## Teleports the rotation and broadcasts the teleport-tagged pose.
func snap_rotation(value: float) -> void:
	if _node == null:
		return
	_node.rotation = value
	_last_sent_rotation = value

	if has_authority:
		_out_position = _node.position
		_out_rotation = value
		_out_scale = _node.scale
		_out_flags |= FLAG_ROTATION | FLAG_TELEPORT
		_pose_dirty = true
	else:
		_head = 0
		_count = 0
		var now: float = Time.get_ticks_usec() / 1000000.0
		_push_snapshot(_node.position, value, Vector2(), now)
		_last_incoming_position = _node.position
		_last_incoming_rotation = value
		_last_incoming_time = now
		_has_incoming_baseline = true
