class_name CYGAuthorityToggle
extends CYGBehaviour
## Runs code on exactly one kind of peer: the view's owner, the room's master
## client, or every peer EXCEPT the owner.
##
## Purely-local gating: it sends nothing and costs zero wire bytes. It derives
## from `CYGBehaviour` only to inherit the spawn/owner-change hooks the view
## walks; it never calls `register_as_observable()`, so it never enters the dirty
## mask, and `has_delta()` is false. Mirrors Unity's `CYGServerToggle.cs`.
##
## Gate semantics (`gate`):
##  OWNER_ONLY: active when this peer owns the view (`view.is_mine`).
##  MASTER_CLIENT_ONLY: active when this peer is the room's master client; scene
##  objects (owner actor 0) gate on it too, matching `CYGView.has_authority`.
##  NOT_OWNER: active on every peer that is not the view's owner — the place for
##  proxy-side effects a client must never run twice.
##
## Active means: nodes in `enable_when_active` are switched on, nodes in
## `disable_when_active` are switched off. Authored local-only logic therefore
## never executes on the wrong peer, because its `_process`/`_physics_process`
## and visibility are off there. Master-client status has no CYGNetwork event, so
## the role is polled behind a cheap change check; spawn and owner-change force
## an early re-evaluation.

enum Gate { OWNER_ONLY, MASTER_CLIENT_ONLY, NOT_OWNER }

## Which peer this toggle treats as "the one".
@export var gate: Gate = Gate.OWNER_ONLY
## NodePaths enabled only while the gate is active (spawners, hit resolution).
@export var enable_when_active: Array[NodePath] = []
## NodePaths disabled while the gate is active (proxy stand-ins, local effects).
@export var disable_when_active: Array[NodePath] = []

var _last_active := false
var _warned_no_view := false


func _ready() -> void:
	_last_active = is_active
	apply()


func cyg_on_spawned() -> void:
	_last_active = is_active
	apply()


func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	_last_active = is_active
	apply()


## True on the peer the gate names; safe to query any time.
var is_active: bool:
	get:
		match gate:
			Gate.OWNER_ONLY:
				var owner_view := view
				return owner_view != null and owner_view.is_mine
			Gate.MASTER_CLIENT_ONLY:
				return CYGNetwork.is_master_client
			Gate.NOT_OWNER:
				var target := view
				return target != null and not target.is_mine
		return false


## Re-evaluates both lists against the current role.
func apply() -> void:
	var target := view
	if target == null:
		if not _warned_no_view:
			_warned_no_view = true
			push_warning("CYGAuthorityToggle on %s has no CYGView in its parent chain; gating is a no-op." % name)
		return
	var active := is_active
	_set_group(enable_when_active, active)
	_set_group(disable_when_active, not active)


## Polls the role behind a change check: the master client flips when the lowest
## actor in the roster changes, and the SDK publishes no event for that.
func _process(_delta: float) -> void:
	var active := is_active
	if active != _last_active:
		_last_active = active
		apply()


func _set_group(nodes: Array[NodePath], enabled: bool) -> void:
	for node_path in nodes:
		var node := get_node_or_null(node_path)
		if node == null or not is_instance_valid(node):
			continue
		# Visual gating only: authority gating is about which peer RUNS code, so
		# collisions stay live for physics ownership reasons the author controls.
		if node is CanvasItem:
			(node as CanvasItem).visible = enabled
		elif node is VisualInstance3D:
			(node as VisualInstance3D).visible = enabled
		# Both exist on every Node, so this always applies.
		node.set_process(enabled)
		node.set_physics_process(enabled)


func has_delta() -> bool:
	return false


## No wire state. Deliberately does not call `super`: this component registers no
## properties, so writing nothing must stay byte-symmetric with reading nothing.
func serialize(_writer: CYGBitPacker) -> void:
	pass


func deserialize(_reader: CYGBitReader) -> void:
	pass
