class_name CYGLagHitbox3D
extends CYGBehaviour
## Per-collider rewind history for 3D lag compensation.
##
## One component per compensable body, attached beside the `CollisionShape3D`
## that defines the hittable volume. Every network tick it records the shape's
## world pose — sphere centre plus radius, or world-axis-aligned box centre plus
## half extents, the same two primitives the Unity and Unreal SDKs rewind — into
## a `CYGHistoryBuffer` keyed by the tick the pose was live on. `CYGLagComp3D`
## reads that history when it rewinds a shot; nothing here queries physics.
##
## Field layout of one recorded `RewindPose3D`:
##
##     tick          i64    the network tick the pose was captured on
##     center        Vector3 world-space sphere centre, or box AABB centre
##     extents       Vector3 box half extents; a sphere carries its radius in x
##     is_sphere     bool   radius test when true, slab AABB test when false
##
## Poses live in a pool of `history_ticks` pre-allocated objects indexed by
## `tick & (capacity - 1)`, so `record_tick()` mutates an existing object and
## writes one reference into the ring: recording allocates nothing after
## `_ready()`. A returned pose object belongs to this hitbox and is overwritten
## by the next capture or the next interpolation — read its fields, do not keep
## the reference across ticks.
##
## Shapes without a rewind primitive (`CapsuleShape3D` and friends) warn once
## and stop recording; `treat_as_sphere` rewinds any supported box as a sphere
## around its centre, mirroring the Unreal buffer's `bTreatAsSphere` flag.
##
## Design derives from Netfox's MIT-licensed rollback and Mirror/UNet's lag
## compensation (attribution in LICENSE-NETFOX.md). Clean-room GDScript; no
## upstream source text is copied.

## Ticks of pose history kept; rounded up to a power of two by the ring.
@export var history_ticks: int = 32
## Capture on the room tick (or, offline, on the injected clock). Turn off to
## drive `record_tick()` from game code, as the headless tests do.
@export var auto_capture: bool = true
## Rewind this body as a sphere even when the shape is a box.
@export var treat_as_sphere: bool = false
## Filter bits used when the nearest `CollisionObject3D` is not a thing this
## hitbox can read (a bare `Node3D`); a query's `layer_mask` ANDs against these.
@export var compensation_layer: int = 1
## Optional explicit path to the `CollisionShape3D`; empty searches the parent's
## children, then this node's own children.
@export var shape_path: NodePath = NodePath("")

## Tick source for the fallback capture cadence; the room clock by default.
var time: CYGNetworkTime = null


## One recorded world pose. Pooled and reused; see the class doc.
class RewindPose3D:
	extends RefCounted
	var tick: int = -1
	var center: Vector3 = Vector3.ZERO
	## Box half extents; a sphere stores its radius in `extents.x` like the
	## Unity/Unreal snapshots do.
	var extents: Vector3 = Vector3.ZERO
	var is_sphere: bool = false

	func lerp_between(a: RewindPose3D, b: RewindPose3D, weight: float) -> void:
		center = a.center.lerp(b.center, weight)
		extents = a.extents.lerp(b.extents, weight)
		is_sphere = a.is_sphere or b.is_sphere


## The index every `CYGLagComp3D` raycasts against, exactly like the Unity SDK's
## static `CYGLagCompensation.buffers` registry: enabling a hitbox (entering the
## tree) joins it, leaving the tree drops it.
static var _registry: Array[CYGLagHitbox3D] = []


var _history: CYGHistoryBuffer = null
var _pool: Array[RewindPose3D] = []
var _mask: int = 0
var _scratch: RewindPose3D = RewindPose3D.new()
var _node_3d: Node3D = null
var _shape: CollisionShape3D = null
var _body_layer: int = 1
var _capture_ready: bool = false
var _last_captured_tick: int = -1
var _drive_by_signal: bool = false
var _owns_time: bool = false
var _ticked: bool = false


static func registry() -> Array[CYGLagHitbox3D]:
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
	_node_3d = get_parent() as Node3D
	if time == null:
		if is_instance_valid(CYGNetwork):
			time = CYGNetwork.network_time
		if time == null:
			# A headless rig or a unit test owns no room clock; a private one keeps
			# the fallback capture cadence running.
			time = CYGNetworkTime.new()
			_owns_time = true
	# Ring capacity is rounded up to a power of two; the pool mirrors it so
	# `tick & _mask` names the same slot on both sides.
	_history = CYGHistoryBuffer.new(maxi(history_ticks, 2))
	var slots: int = _history.capacity()
	_mask = slots - 1
	_pool.resize(slots)
	for i in range(slots):
		_pool[i] = RewindPose3D.new()
	_resolve_shape()
	_body_layer = _resolve_layer()
	_capture_ready = _shape != null and _node_3d != null and _supported_shape(_shape)
	if not _capture_ready:
		push_warning("CYGLagHitbox3D on %s: no rewindable CollisionShape3D found; this body cannot be hit by lag compensation." % name)
	if auto_capture and is_instance_valid(CYGNetwork) and not _owns_time:
		CYGNetwork.network_tick.connect(_on_network_tick)
		_drive_by_signal = true
		_ticked = true
	set_physics_process(auto_capture and not _drive_by_signal)


## Binds the shape at runtime, e.g. when a rig builds its collider after the
## networked object spawns. Clears any history recorded under the old shape.
func use_shape(shape: CollisionShape3D) -> void:
	if shape == null or shape == _shape:
		return
	_shape = shape
	_body_layer = _resolve_layer()
	_capture_ready = shape != null and _node_3d != null and _supported_shape(shape)
	_last_captured_tick = -1
	_history.clear()


## A fresh spawn starts a fresh history: the old ticks belonged to a different
## owner's replication stream.
func cyg_on_spawned() -> void:
	if _history != null:
		_history.clear()
	_last_captured_tick = -1


## The nearest view's id, or -1 offline; `CYGLagComp3D` uses it to skip the
## shooter's own body and to focus a query on one expected target.
func get_view_id() -> int:
	var target := view
	return target.view_id if target != null else -1


## The bits a query's `layer_mask` filters against.
func layer_bits() -> int:
	return _body_layer


## Whether this hitbox currently takes part in rewind queries: shape bound,
## enabled and in the tree. Visibility is NOT required — a hidden body whose
## collider is enabled is still physically hittable in Godot.
func is_queryable() -> bool:
	if not _capture_ready or not is_inside_tree():
		return false
	return _shape != null and is_instance_valid(_shape) and not _shape.disabled


## Records the parent's current world pose under `value_tick`. Cheap enough to
## call every tick; writes into the pooled object for the tick's slot, so it
## allocates nothing. The pose slot silently reuses the entry it displaces,
## exactly like the ring underneath it.
func record_tick(value_tick: int) -> void:
	if not _capture_ready or value_tick < 0:
		return
	if _shape == null or not is_instance_valid(_shape) or _shape.disabled:
		return
	var slot: int = value_tick & _mask
	var pose: RewindPose3D = _pool[slot]
	_fill_pose(pose, value_tick)
	_history.push(value_tick, pose)
	_last_captured_tick = value_tick


## The pose this body held at `value_tick`, clamped and interpolated like the
## reference buffers: exact tick wins; earlier than the history clamps to the
## oldest pose; newer than it (the normal case for the current tick, capture
## trails the clock) clamps to the newest; in between, the bracketing poses are
## lerped and `tick` is stamped with the requested value. Null when nothing has
## been recorded yet — a proxy's replica that has received no delta simply has
## no history, so a shot cannot hit it before its first pose arrives.
func sample_pose(value_tick: int) -> RewindPose3D:
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
	# Bracket scan: every probe is an O(1) `contains()`, the span never exceeds
	# the ring, and nothing is allocated; `_scratch` is a reusable member.
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
	var from_pose: RewindPose3D = _history.get_state(lo)
	var to_pose: RewindPose3D = _history.get_state(hi)
	var weight: float = float(value_tick - lo) / float(hi - lo)
	_scratch.lerp_between(from_pose, to_pose, weight)
	_scratch.tick = value_tick
	return _scratch


## Oldest tick still rewindable, -1 when empty.
func oldest_tick() -> int:
	return _history.oldest_tick() if _history != null else -1


## Newest tick still rewindable, -1 when empty.
func newest_tick() -> int:
	return _history.newest_tick() if _history != null else -1


## ------------------------------------------------------------------- internals

func _on_network_tick(value_tick: int) -> void:
	if auto_capture:
		record_tick(value_tick)


## Fallback cadence when no room tick pump exists: advance the private clock,
## capture once per tick change.
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
		_shape = get_node_or_null(shape_path) as CollisionShape3D
		return
	if _node_3d != null:
		for child in _node_3d.get_children():
			if child is CollisionShape3D:
				_shape = child
				return
	for child in get_children():
		if child is CollisionShape3D:
			_shape = child
			return


func _resolve_layer() -> int:
	# Walk up from the shape's owner: a `CollisionObject3D` parent (body or
	# area) carries the authoritative `collision_layer`; bare nodes fall back
	# to the exported bits.
	var node := _shape as Node
	while node != null:
		var collider := node as CollisionObject3D
		if collider != null:
			return collider.collision_layer
		node = node.get_parent()
	return compensation_layer


func _supported_shape(shape: CollisionShape3D) -> bool:
	if shape == null or not is_instance_valid(shape):
		return false
	return shape.shape is BoxShape3D or shape.shape is SphereShape3D


func _fill_pose(pose: RewindPose3D, value_tick: int) -> void:
	pose.tick = value_tick
	var xf := _shape.global_transform
	var resource := _shape.shape
	if resource is SphereShape3D:
		pose.is_sphere = true
		pose.center = xf.origin
		pose.extents = Vector3((resource as SphereShape3D).radius, 0.0, 0.0)
		return
	if resource is BoxShape3D:
		var half: Vector3 = (resource as BoxShape3D).size * 0.5
		if treat_as_sphere:
			pose.is_sphere = true
			pose.center = xf.origin
			pose.extents = Vector3(maxf(half.x, maxf(half.y, half.z)), 0.0, 0.0)
			return
		pose.is_sphere = false
		var world: AABB = xf * AABB(-half, half * 2.0)
		pose.center = world.get_center()
		pose.extents = world.size * 0.5
		return
	# Unreachable via `_supported_shape`; keep the last pose rather than invent one.
	pose.tick = _last_captured_tick
