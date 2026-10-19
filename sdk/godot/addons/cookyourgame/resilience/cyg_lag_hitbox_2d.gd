class_name CYGLagHitbox2D
extends CYGBehaviour
## Per-collider rewind history for 2D lag compensation.
##
## The `PhysicsSpace2D` sibling of `CYGLagHitbox3D`: attached beside the
## `CollisionShape2D` that defines the hittable volume, it records the shape's
## world pose — circle centre plus radius, or oriented-box centre, half extents
## and rotation angle — once per network tick into a `CYGHistoryBuffer`, and
## `CYGLagComp2D` rewinds shots through `sample_pose()`.
##
## Field layout of one recorded `RewindPose2D`:
##
##     tick        i64      the network tick the pose was captured on
##     center      Vector2  world-space circle/box centre
##     extents     Vector2  box half extents in the shape's own axes; a circle
##                          carries its radius in x
##     angle       float    the shape's world rotation, so the box rewinds
##                          oriented, not axis-aligned
##     is_circle   bool     circle test when true, oriented-box slab test when
##                          false
##
## Poses come from a pre-allocated pool indexed by `tick & (capacity - 1)`;
## recording mutates the pooled object and never allocates. A returned pose
## belongs to this hitbox and is overwritten by the next capture or
## interpolation — read the fields, do not hold the reference.
##
## Only `CircleShape2D` and `RectangleShape2D` are rewindable (the convex
## polygon shapes have no cheap exact rewind test); anything else warns once and
## stops recording.
##
## Design derives from Netfox's MIT-licensed rollback and Mirror/UNet's lag
## compensation (attribution in LICENSE-NETFOX.md). Independent GDScript; no
## upstream source text is copied.

## Ticks of pose history kept; rounded up to a power of two by the ring.
@export var history_ticks: int = 32
## Capture on the room tick (or, offline, on the injected clock).
@export var auto_capture: bool = true
## Rewind this body as a circle even when the shape is a box.
@export var treat_as_circle: bool = false
## Filter bits used when no `CollisionObject2D` owns the shape.
@export var compensation_layer: int = 1
## Optional explicit path to the `CollisionShape2D`; empty searches the parent's
## children, then this node's own children.
@export var shape_path: NodePath = NodePath("")

## Tick source for the fallback capture cadence; the room clock by default.
var time: CYGNetworkTime = null


## One recorded world pose. Pooled and reused; see the class doc.
class RewindPose2D:
	extends RefCounted
	var tick: int = -1
	var center: Vector2 = Vector2.ZERO
	var extents: Vector2 = Vector2.ZERO
	var angle: float = 0.0
	var is_circle: bool = false

	func lerp_between(a: RewindPose2D, b: RewindPose2D, weight: float) -> void:
		center = a.center.lerp(b.center, weight)
		extents = a.extents.lerp(b.extents, weight)
		# Shortest-arc angle blend so a spin crossing +/-pi does not unwind.
		var delta: float = wrapf(b.angle - a.angle + PI, 0.0, TAU) - PI
		angle = a.angle + delta * weight
		is_circle = a.is_circle or b.is_circle


## The index every `CYGLagComp2D` raycasts against; entering the tree joins it,
## leaving drops it, exactly like the Unity SDK's static buffer registry.
static var _registry: Array[CYGLagHitbox2D] = []


var _history: CYGHistoryBuffer = null
var _pool: Array[RewindPose2D] = []
var _mask: int = 0
var _scratch: RewindPose2D = RewindPose2D.new()
var _node_2d: Node2D = null
var _shape: CollisionShape2D = null
var _body_layer: int = 1
var _capture_ready: bool = false
var _last_captured_tick: int = -1
var _drive_by_signal: bool = false
var _owns_time: bool = false
var _ticked: bool = false


static func registry() -> Array[CYGLagHitbox2D]:
	return _registry


static func clear_registry() -> void:
	_registry.clear()


func _enter_tree() -> void:
	if not _registry.has(self):
		_registry.append(self)


func _exit_tree() -> void:
	_registry.erase(self)
	if _ticked and is_instance_valid(CYGNetwork):
		CYGNetwork.network_tick.disconnect(_on_network_tick)
	_ticked = false
	super._exit_tree()


func _ready() -> void:
	_node_2d = get_parent() as Node2D
	if time == null:
		if is_instance_valid(CYGNetwork):
			time = CYGNetwork.network_time
		if time == null:
			time = CYGNetworkTime.new()
			_owns_time = true
	_history = CYGHistoryBuffer.new(maxi(history_ticks, 2))
	var slots: int = _history.capacity()
	_mask = slots - 1
	_pool.resize(slots)
	for i in range(slots):
		_pool[i] = RewindPose2D.new()
	_resolve_shape()
	_body_layer = _resolve_layer()
	_capture_ready = _shape != null and _node_2d != null and _supported_shape(_shape)
	if not _capture_ready:
		push_warning("CYGLagHitbox2D on %s: no rewindable CollisionShape2D found; this body cannot be hit by lag compensation." % name)
	if auto_capture and is_instance_valid(CYGNetwork) and not _owns_time:
		CYGNetwork.network_tick.connect(_on_network_tick)
		_drive_by_signal = true
		_ticked = true
	set_physics_process(auto_capture and not _drive_by_signal)


## Binds the shape at runtime; clears history recorded under the old shape.
func use_shape(shape: CollisionShape2D) -> void:
	if shape == null or shape == _shape:
		return
	_shape = shape
	_body_layer = _resolve_layer()
	_capture_ready = _node_2d != null and _supported_shape(shape)
	_last_captured_tick = -1
	_history.clear()


func cyg_on_spawned() -> void:
	if _history != null:
		_history.clear()
	_last_captured_tick = -1


## The nearest view's id, or -1 offline.
func get_view_id() -> int:
	var target := view
	return target.view_id if target != null else -1


## The bits a query's `layer_mask` filters against.
func layer_bits() -> int:
	return _body_layer


## Whether this hitbox takes part in rewind queries.
func is_queryable() -> bool:
	if not _capture_ready or not is_inside_tree():
		return false
	return _shape != null and is_instance_valid(_shape) and not _shape.disabled


## Records the shape's current world pose under `value_tick`; allocation-free.
func record_tick(value_tick: int) -> void:
	if not _capture_ready or value_tick < 0:
		return
	if _shape == null or not is_instance_valid(_shape) or _shape.disabled:
		return
	var slot: int = value_tick & _mask
	var pose: RewindPose2D = _pool[slot]
	_fill_pose(pose, value_tick)
	_history.push(value_tick, pose)
	_last_captured_tick = value_tick


## The pose held at `value_tick`: exact tick wins, earlier clamps to the oldest,
## newer clamps to the newest, in between the bracket is lerped (angle takes the
## shortest arc). Null while nothing has been recorded — a replica with no
## deltas yet cannot be hit.
func sample_pose(value_tick: int) -> RewindPose2D:
	if _history == null or _history.size() == 0:
		return null
	var exact: Variant = _history.get_state(value_tick)
	if exact != null:
		return exact
	var oldest: int = _history.oldest_tick()
	var newest: int = _history.newest_tick()
	if oldest < 0:
		return null
	if value_tick <= oldest:
		return _history.get_state(oldest)
	if value_tick >= newest:
		return _history.get_state(newest)
	var lo: int = value_tick
	while lo > oldest and not _history.contains(lo):
		lo -= 1
	var hi: int = value_tick + 1
	while hi < newest and not _history.contains(hi):
		hi += 1
	if not _history.contains(lo):
		return _history.get_state(oldest)
	if not _history.contains(hi):
		return _history.get_state(newest)
	var from_pose: RewindPose2D = _history.get_state(lo)
	var to_pose: RewindPose2D = _history.get_state(hi)
	var weight: float = float(value_tick - lo) / float(hi - lo)
	_scratch.lerp_between(from_pose, to_pose, weight)
	_scratch.tick = value_tick
	return _scratch


func oldest_tick() -> int:
	return _history.oldest_tick() if _history != null else -1


func newest_tick() -> int:
	return _history.newest_tick() if _history != null else -1


## ------------------------------------------------------------------- internals

func _on_network_tick(value_tick: int) -> void:
	if auto_capture:
		record_tick(value_tick)


func _physics_process(delta: float) -> void:
	if _drive_by_signal or not auto_capture:
		return
	if _owns_time:
		time.advance(delta * 1000.0)
	var value_tick: int = time.tick()
	if value_tick != _last_captured_tick:
		record_tick(value_tick)


func _resolve_shape() -> void:
	if not shape_path.is_empty():
		_shape = get_node_or_null(shape_path) as CollisionShape2D
		return
	if _node_2d != null:
		for child in _node_2d.get_children():
			if child is CollisionShape2D:
				_shape = child
				return
	for child in get_children():
		if child is CollisionShape2D:
			_shape = child
			return


func _resolve_layer() -> int:
	var node := _shape as Node
	while node != null:
		var collider := node as CollisionObject2D
		if collider != null:
			return collider.collision_layer
		node = node.get_parent()
	return compensation_layer


func _supported_shape(shape: CollisionShape2D) -> bool:
	if shape == null or not is_instance_valid(shape):
		return false
	return shape.shape is CircleShape2D or shape.shape is RectangleShape2D


func _fill_pose(pose: RewindPose2D, value_tick: int) -> void:
	pose.tick = value_tick
	pose.center = _shape.global_position
	pose.angle = _shape.global_rotation
	var resource := _shape.shape
	if resource is CircleShape2D:
		pose.is_circle = true
		pose.extents = Vector2((resource as CircleShape2D).radius, 0.0)
		return
	if resource is RectangleShape2D:
		var half: Vector2 = (resource as RectangleShape2D).size * 0.5
		if treat_as_circle:
			pose.is_circle = true
			pose.extents = Vector2(maxf(half.x, half.y), 0.0)
		else:
			pose.is_circle = false
			pose.extents = half
		return
	# Unreachable via `_supported_shape`; keep the last pose rather than invent one.
	pose.tick = _last_captured_tick
