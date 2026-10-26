class_name CYGNodePool
extends Node
## Zero-allocation node recycling for the objects a match churns through:
## bullets, muzzle flashes, hit VFX, damage numbers.
##
## Local-only by design: this class writes no bytes. It is the recycling layer
## under the networked event components, which already avoid allocating per
## event. A recycled node that also carries a `CYGView` is a different matter:
## `CYGNetwork` assigns a fresh view id on every spawn and frees the node on
## despawn, so networked objects only become poolable through a spawn path that
## re-assigns ids to a live instance. Until that exists, pool the local half of
## a bullet (mesh, trail, impact decal) and keep the view on `CYGNetwork`.
##
## The reuse contract, in three sentences: `reserve()` pays the allocation cost
## up front and every `spawn()` after that only re-parents an existing node;
## `despawn()` hands the node back to the idle list, and the pool owns its
## `visible`/process flags while it is there; `release()` opts a node out of the
## contract and frees it — with `release_key()`/`clear()` and the failed-`spawn()`
## path, those are the only places this class calls `queue_free()`.
##
## Nothing survives a scene change by magic: park the pool under a persistent
## root (the `CYGNetwork` autoload, as `CYGSceneManager` does) so its idle
## children are not part of the scene being freed. Live instances in the outgoing
## scene die with it, and `_on_instance_exiting()` forgets each one on the spot
## and re-parks it a frame later only if it turns out to have been unparented
## rather than reaped — so the pool never holds a dead reference and never
## re-parents a node the engine is already deleting.

enum ExhaustedPolicy {
	## Instantiate one more on demand: a burst of events never loses an object.
	GROW,
	## Hand back `null` and let the caller decide: the cap then really is a cap.
	RETURN_NULL,
}

## Optional hooks a pooled scene may implement; called on the instance root.
const HOOK_SPAWNED := "cyg_pool_spawned"
const HOOK_DESPAWNED := "cyg_pool_despawned"
## Keys are dictionary lookups on every spawn; a runaway name is a bug, not a cost.
const MAX_KEY_LENGTH := 128

## Give back an instance that leaves the tree without a `despawn()` call when it
## turns out to still be alive. Off means such an instance is forgotten and left
## to whatever still owns it.
@export var auto_recycle: bool = true
## What `spawn()` does when the key has no idle instance.
@export var on_exhausted: ExhaustedPolicy = ExhaustedPolicy.GROW
## Ceiling on instances created per key, idle and live counted together.
## 0 is unbounded, which only makes sense next to `GROW`.
@export var max_per_key: int = 0
## Fallback parent for `spawn()`; the pool's own parent when left unset.
@export var default_parent: Node = null
## Defer every registered key's reserve to `_ready` instead of building in
## `register()`, so a level can batch its warm-up behind a loading screen.
@export var prewarm_on_ready: bool = false

var _scenes: Dictionary = {}  # key -> PackedScene
var _targets: Dictionary = {}  # key -> int reserve target
var _idle: Dictionary = {}  # key -> Array of idle instances, children of self
var _live: Dictionary = {}  # instance id -> key
var _stats: Dictionary = {}  # key -> {created, spawns, misses, grew, released, live}
## Non-zero while this node re-parents one of its own instances: the move makes
## the node leave its parent, and `_on_instance_exiting` must not interpret that
## as the game throwing the object away.
var _moving := 0


func _ready() -> void:
	if prewarm_on_ready:
		prewarm()


func _exit_tree() -> void:
	# The pool is going away: drop the tables so a deferred reclaim cannot reach
	# into a half-torn-down instance. Idle children are freed with this node.
	_live.clear()
	_idle.clear()
	_stats.clear()


## ------------------------------------------------------------------ registration

## Declares a key. `reserve_count` instances are built immediately unless
## `prewarm_on_ready` defers the work to `_ready`.
func register(key: String, scene: PackedScene, reserve_count: int = 0) -> bool:
	if key.is_empty() or scene == null or key.length() > MAX_KEY_LENGTH:
		push_error("CYGNodePool.register: '%s' needs a key and a PackedScene." % key)
		return false
	_scenes[key] = scene
	_targets[key] = maxi(reserve_count, 0)
	if not _idle.has(key):
		_idle[key] = []
		_stats[key] = {"created": 0, "spawns": 0, "misses": 0, "grew": 0, "released": 0, "live": 0}
	# Registering mid-match is legal, so honour the count here too; the matching
	# `prewarm()` then finds nothing to top up and costs nothing.
	if reserve_count > 0 and not prewarm_on_ready:
		reserve(key, reserve_count)
	return true


## Builds `count` instances for `key` and parks them. Returns how many were made,
## fewer than asked when `max_per_key` cuts the reserve short.
func reserve(key: String, count: int) -> int:
	var scene: PackedScene = _scenes.get(key)
	if scene == null:
		push_error("CYGNodePool: cannot reserve %d for unregistered key '%s'." % [count, key])
		return 0
	var nodes: Array = _idle.get(key)
	var built := 0
	for i in range(count):
		if _at_cap(key):
			break
		var instance := scene.instantiate()
		if instance == null:
			push_error("CYGNodePool: '%s' has no Node root." % scene.resource_path)
			break
		_bump(key, "created")
		_park(instance)
		add_child(instance)
		_ensure_exit_hook(instance)
		nodes.append(instance)
		built += 1
	return built


## Tops every registered key up to the count it was registered with.
func prewarm() -> int:
	var built := 0
	for key in _targets.keys():
		var nodes: Array = _idle.get(key)
		var wanted := int(_targets[key])
		if nodes == null or nodes.size() >= wanted:
			continue
		built += reserve(key as String, wanted - nodes.size())
	return built


## ----------------------------------------------------------------- acquire/return

## Takes an instance for `key` and parents it into `target_parent`. The node is
## in the tree when this returns, so the caller sets its pose afterwards.
## Returns `null` for an unregistered key or when the policy refuses to grow.
func spawn(key: String, target_parent: Node = null) -> Node:
	var nodes: Array = _idle.get(key)
	var instance: Node = null
	if nodes != null and not nodes.is_empty():
		instance = nodes.pop_back() as Node
	else:
		_bump(key, "misses")
		var scene: PackedScene = _scenes.get(key)
		if scene == null:
			push_error("CYGNodePool: no prefab registered for '%s'." % key)
			return null
		if on_exhausted == ExhaustedPolicy.RETURN_NULL or _at_cap(key):
			return null
		instance = scene.instantiate()
		if instance == null:
			push_error("CYGNodePool: '%s' has no Node root." % scene.resource_path)
			return null
		_bump(key, "created")
		_bump(key, "grew")
		_ensure_exit_hook(instance)
	var parent := _resolve_parent(target_parent)
	if parent == null:
		push_error("CYGNodePool: nowhere to attach '%s'; the instance is freed." % key)
		# Freed directly: an instance taken from the idle list is no longer in it,
		# so `release()` would refuse it as unmanaged and leave it orphaned.
		release_unmanaged(instance)
		return null
	_unpark(instance)
	_moving += 1
	_attach(instance, parent)
	_moving -= 1
	_live[instance.get_instance_id()] = key
	_bump(key, "live", 1)
	_bump(key, "spawns")
	if instance.has_method(HOOK_SPAWNED):
		instance.call(HOOK_SPAWNED)
	return instance


## Bullets and VFX nearly always need a pose the moment they appear.
func spawn_at_3d(key: String, world_position: Vector3, target_parent: Node = null) -> Node:
	var instance := spawn(key, target_parent)
	var node_3d := instance as Node3D
	if node_3d != null:
		node_3d.global_position = world_position
	return instance


func spawn_at_2d(key: String, world_position: Vector2, target_parent: Node = null) -> Node:
	var instance := spawn(key, target_parent)
	var node_2d := instance as Node2D
	if node_2d != null:
		node_2d.global_position = world_position
	return instance


## Returns an instance to the idle list. Safe to call from the instance's own
## collision or timeout code: nothing is freed, the node is only re-parented.
func despawn(node: Node) -> bool:
	if node == null or not is_instance_valid(node):
		return false
	var id := node.get_instance_id()
	var key: Variant = _live.get(id)
	if key == null:
		push_warning("CYGNodePool.despawn: '%s' was not spawned by this pool; it stays with its caller." % node.name)
		return false
	_live.erase(id)
	_bump(key as String, "live", -1)
	if node.has_method(HOOK_DESPAWNED):
		node.call(HOOK_DESPAWNED)
	_moving += 1
	_park(node)
	_return_to_idle(key as String, node)
	_moving -= 1
	return true


## Opts an instance out of the contract: it is freed and never re-parked. Idle
## instances are accepted too, which is how a game shrinks an oversized reserve.
func release(node: Node) -> bool:
	if node == null or not is_instance_valid(node):
		return false
	var key: Variant = _live.get(node.get_instance_id())
	if key != null:
		_live.erase(node.get_instance_id())
		_bump(key as String, "live", -1)
	else:
		var owner_key := _key_of_idle(node)
		if owner_key.is_empty():
			push_warning("CYGNodePool.release: '%s' is not managed by this pool." % node.name)
			return false
		key = owner_key
		var nodes: Array = _idle.get(key)
		nodes.erase(node)
	_bump(key as String, "released")
	# Deferred even for an idle instance: `despawn()` callers can be inside the
	# node's own physics callback, and freeing there is illegal.
	node.queue_free()
	return true


## Frees a node without consulting the tables: the escape hatch for an instance
## that left the pool's books (a released key, a failed `spawn()`).
func release_unmanaged(node: Node) -> void:
	if node != null and is_instance_valid(node):
		node.queue_free()


## Frees every idle instance of a key (live ones keep running) and forgets the
## registration, so a level that no longer needs an effect stops paying for it.
func release_key(key: String) -> int:
	var nodes: Array = _idle.get(key)
	if nodes == null:
		return 0
	var freed := 0
	while not nodes.is_empty():
		var node := nodes.pop_back() as Node
		if node != null and is_instance_valid(node):
			_bump(key, "released")
			node.queue_free()
			freed += 1
	_scenes.erase(key)
	_targets.erase(key)
	_idle.erase(key)
	return freed


## Frees every idle instance of every key.
func clear() -> void:
	for key in _idle.keys():
		release_key(key as String)


## ------------------------------------------------------------------------ reporting

## Counters for one key, as a copy the caller may keep.
func stats_for(key: String) -> Dictionary:
	var entry: Variant = _stats.get(key)
	if entry == null:
		return {}
	return (entry as Dictionary).duplicate(true)


func idle_count(key: String) -> int:
	var nodes: Array = _idle.get(key)
	return 0 if nodes == null else nodes.size()


func live_count(key: String) -> int:
	if not _stats.has(key):
		return 0
	var entry: Dictionary = _stats[key]
	return int(entry.get("live", 0))


## Instances built for a key, whether idle, live or since freed; the number only
## moves down through `release()`, so it is how a leak shows up as growth.
func created_count(key: String) -> int:
	if not _stats.has(key):
		return 0
	var entry: Dictionary = _stats[key]
	return int(entry.get("created", 0))


## ----------------------------------------------------------------------- internals

func _resolve_parent(target_parent: Node) -> Node:
	if target_parent != null and target_parent.is_inside_tree():
		return target_parent
	if default_parent != null and is_instance_valid(default_parent) and default_parent.is_inside_tree():
		return default_parent
	# The pool's own parent is the level root when the pool is parked under an
	# autoload, which is exactly where recycled objects belong.
	var parent := get_parent()
	return parent if parent != null and parent.is_inside_tree() else null


## `add_child()` on a node that already has a parent is an error, and a pooled
## instance always has one: itself when idle, or the previous parent when a game
## reparented it by hand instead of calling `despawn()`.
func _attach(instance: Node, parent: Node) -> void:
	var previous := instance.get_parent()
	if previous == parent:
		return
	if previous == null:
		parent.add_child(instance)
	else:
		# `keep_global_transform` off on purpose: the caller re-poses a pooled
		# instance, and preserving the old pose costs a basis solve.
		instance.reparent(parent, false)


## Connects the instance's `tree_exiting` exactly once. Per-spawn (dis)connection
## would be an allocation in the hot path, and the id travels by value because a
## bound object reference would dangle on the frame the instance dies.
func _ensure_exit_hook(instance: Node) -> void:
	var bound := _on_instance_exiting.bind(instance.get_instance_id())
	if not instance.is_connected("tree_exiting", bound):
		instance.connect("tree_exiting", bound)


## Pooled instances must be inert while idle; the pose reset is the caller's
## because it is also the spawn position.
func _park(node: Node) -> void:
	if node is CanvasItem:
		(node as CanvasItem).visible = false
	elif node is VisualInstance3D:
		(node as VisualInstance3D).visible = false
	node.set_process(false)
	node.set_physics_process(false)
	node.set_process_internal(false)
	node.set_physics_process_internal(false)


func _unpark(node: Node) -> void:
	if node is CanvasItem:
		(node as CanvasItem).visible = true
	elif node is VisualInstance3D:
		(node as VisualInstance3D).visible = true
	node.set_process(true)
	node.set_physics_process(true)


func _return_to_idle(key: String, node: Node) -> void:
	var nodes: Array = _idle.get(key)
	if nodes == null:
		# The key was released while the instance was live: honour that rather
		# than resurrect a registration the game asked to forget.
		node.queue_free()
		return
	nodes.append(node)
	# `reparent()` needs the node to be in the tree, and a reclaimed node has just
	# left it, so an orphan is adopted rather than moved.
	var previous := node.get_parent()
	if previous == self:
		return
	if previous == null:
		add_child(node)
	else:
		node.reparent(self, false)


func _key_of_idle(node: Node) -> String:
	for key in _idle.keys():
		var nodes: Array = _idle[key]
		if node in nodes:
			return key as String
	return ""


func _at_cap(key: String) -> bool:
	if max_per_key <= 0:
		return false
	return idle_count(key) + live_count(key) >= max_per_key


func _bump(key: String, field: String, amount: int = 1) -> void:
	var entry: Dictionary = _stats.get(key)
	# A key with no table (a released one) has nowhere to accumulate; counters are
	# diagnostics and must never be the reason an event gets dropped.
	if entry != null:
		entry[field] = int(entry.get(field, 0)) + amount


func _on_instance_exiting(id: int) -> void:
	if _moving > 0:
		return
	var key: Variant = _live.get(id)
	if key == null:
		return
	_live.erase(id)
	_bump(key as String, "live", -1)
	# It left the tree with no `despawn()` call: game code freed it, or a scene
	# change is reaping the level. Which one is only knowable next frame.
	call_deferred("_reclaim_or_forget", id, key)


func _reclaim_or_forget(id: int, key: Variant) -> void:
	var node := instance_from_id(id) as Node
	if node == null or node.is_queued_for_deletion():
		return
	if not auto_recycle or not _idle.has(key):
		return
	_park(node)
	_moving += 1
	_return_to_idle(key as String, node)
	_moving -= 1
