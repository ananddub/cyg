class_name CYGVisibilityCull
extends CYGBehaviour
## Network dormancy and spatial visibility culling for a `CYGView`.
##
## A distance rule evaluated against this peer's `CYGNetwork.local_player` avatar
## (the Godot shape of Unity's `CYGVisibility.cs` distance rule). Beyond
## `cull_distance` the view goes DORMANT: `has_delta()` reports false so this
## component contributes nothing to the dirty mask, and — when the SDK build's
## `CYGView` carries a `network_dormant` flag — the whole view is marked so the
## tick pump (`flush_view_deltas`) can skip it entirely. Re-entering range wakes
## the view, forces `mark_all_dirty()` so the observer gets one full refresh
## instead of a stale resume, and re-animates. Hysteresis: a view must travel
## `hysteresis` past the entry radius before its state flips, so a body
## oscillating at the boundary cannot flap. `always_networked` is the escape
## hatch for players and bosses, which must never dorm.
##
## Sender vs receiver: deltas fan out from the view's AUTHORITY peer, so only
## that peer's evaluation suppresses traffic; on proxies the same evaluation
## drives the visual half (hide/show renderers) and `dormancy_changed`, which the
## game can use to gate interpolation or effects. Per-observer network interest
## would need a relay-side subscription API; distance-to-local-player gating on
## the authority is what this protocol supports, and master-client-owned scene
## objects should keep `always_networked` unless the master is the only observer
## that matters.
##
## No wire state of its own: it registers no sync properties, so its base
## serialize/deserialize write and read nothing; it is observable only so the
## dormancy gate has a `has_delta()` to clamp.

## Re-entry radius for observers farther in than this. 0 disables culling.
@export var cull_distance: float = 50.0
## Extra distance an already-near view may travel before it exits to dormant.
@export var hysteresis: float = 5.0
## Players, bosses, anything the game must track at any range: never dorms.
@export var always_networked: bool = false
## Hide `CanvasItem`/`VisualInstance3D` children while out of range.
@export var hide_visuals: bool = true
## NodePath of a `Node3D` to measure from; empty falls back to this peer's own
## networked `Node3D` (the local avatar), re-searched until one is found.
@export var observer_path: NodePath = NodePath()
## Seconds between evaluations; visibility is a slow-moving fact.
@export var evaluate_interval: float = 0.25

signal dormancy_changed(dormant: bool)

## True while the target is inside the (hysteresis-aware) observer range.
var in_range := true
## True while outgoing state is suppressed; only ever set on the authority peer.
var is_dormant := false

var _accum := 0.0
var _observer: Node3D = null
var _target: Node3D = null
var _visuals: Array[Node] = []
var _visuals_cached := false
var _dormancy_supported := false
var _dormancy_probed := false


func _ready() -> void:
	var target := view
	if target != null:
		_target = target.network_node as Node3D
		if _target == null:
			push_warning("CYGVisibilityCull on %s: network node is not a Node3D; distance rule cannot run." % name)
	# Observable so `has_delta()` is part of the view's mask evaluation; with no
	# registered properties the bit stays clear and no bytes are written.
	register_as_observable()


func cyg_on_spawned() -> void:
	_visuals_cached = false
	_dormancy_probed = false
	in_range = true
	_set_dormant(false)


func cyg_on_despawned() -> void:
	_set_dormant(false)


func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	# The new authority may sit on the other side of the rule; re-probe and let
	# the next evaluation re-derive the dormancy state from scratch.
	_dormancy_probed = false
	in_range = true
	_set_dormant(false)


## Force the view out of dormancy now (respawn cutscenes, teleport scripts).
func force_wake() -> void:
	in_range = true
	_set_dormant(false)


func _process(delta: float) -> void:
	if _target == null:
		return
	_accum += delta
	if _accum < evaluate_interval:
		return
	_accum = 0.0

	var near := true
	if not always_networked:
		var observer := _resolve_observer()
		if observer == null or not is_instance_valid(observer):
			# Fail open: no local player yet (lobby, pre-join) must not blackout state.
			near = true
		else:
			var radius := cull_distance if not in_range else cull_distance + hysteresis
			near = observer.global_position.distance_squared_to(_target.global_position) <= radius * radius
	if near == in_range:
		return
	in_range = near
	_apply_visuals(near)
	if near:
		_set_dormant(false)
	elif has_authority:
		# Only the authority peer has outgoing state to suppress; a proxy flipping
		# dormant would falsely mark its own inbound stream dead.
		_set_dormant(true)


## Outgoing gate: while dormant this component reports no delta, so its bit in
## the view's dirty mask stays clear no matter what it would have sampled.
func has_delta() -> bool:
	if is_dormant:
		return false
	return super.has_delta()


func _set_dormant(dormant: bool) -> void:
	if dormant == is_dormant:
		return
	is_dormant = dormant
	_probe_dormancy_support()
	if _dormancy_supported:
		view.set("network_dormant", dormant)
	if not dormant:
		# State dirtied while out of range kept its dirty bits (serialization never
		# ran to clear them); the wake refreshes everything so the observer that
		# just found this object on screen receives one complete snapshot stream.
		_mark_all_dirty()
	dormancy_changed.emit(dormant)


## `CYGView` gains its dormancy flag through a guarded runtime property probe, so
## this component compiles and runs against SDK builds that have not added it.
func _probe_dormancy_support() -> void:
	if _dormancy_probed:
		return
	var target := view
	if target == null:
		return
	_dormancy_probed = true
	for property in target.get_property_list():
		if str(property["name"]) == "network_dormant":
			_dormancy_supported = true
			return


func _mark_all_dirty() -> void:
	if _properties != null and _properties.size() > 0:
		_properties.mark_all_dirty()


func _resolve_observer() -> Node3D:
	if observer_path != NodePath():
		if _observer == null or not is_instance_valid(_observer):
			_observer = get_node_or_null(observer_path) as Node3D
		return _observer
	if _observer != null and is_instance_valid(_observer):
		return _observer
	# Fallback: this peer's own networked Node3D — the view it owns. Re-searched
	# only at the evaluate interval (the Array copy here is interval-bound, not
	# per-frame) and cached once the local avatar spawns.
	var local := CYGNetwork.local_player
	if local == null:
		return null
	for entry in CYGNetwork.all_views():
		var candidate := entry as CYGView
		if candidate == null or not candidate.is_mine:
			continue
		var node := candidate.network_node as Node3D
		if node != null:
			_observer = node
			return node
	return null


## Visual half of the rule, evaluated on every peer: hide the object's direct
## renderer children while culled so distant avatars cost no draw calls.
func _apply_visuals(show: bool) -> void:
	if not hide_visuals or _target == null:
		return
	if not _visuals_cached:
		_visuals.clear()
		for child in _target.get_children():
			if child is CanvasItem or child is VisualInstance3D:
				_visuals.append(child)
		_visuals_cached = true
	for node in _visuals:
		if not is_instance_valid(node):
			continue
		if node is CanvasItem:
			(node as CanvasItem).visible = show
		elif node is VisualInstance3D:
			(node as VisualInstance3D).visible = show


## No wire state; symmetric no-op pair, same contract as `CYGOwnershipToggle`.
func serialize(_writer: CYGBitPacker) -> void:
	pass


func deserialize(_reader: CYGBitReader) -> void:
	pass
