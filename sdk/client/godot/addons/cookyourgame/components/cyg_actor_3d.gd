class_name CYGActor3D
extends Node3D
## Base avatar node for a networked 3D object: guarantees a `CYGView` and turns
## the view's signals into overridable methods.
##
## Derive from this instead of wiring a `CYGView` by hand. The view is found (or
## created as the child "CYGView") on first access, registers itself with
## `CYGNetwork`, and its `spawned` / `despawned` / `owner_changed` signals are
## forwarded to `cyg_on_spawned` / `cyg_on_despawned` / `cyg_on_owner_changed`,
## with `cyg_on_start_local` firing once for the owning peer.

var _view: CYGView = null
var _start_local_fired := false


## Network identity of this actor; created as the "CYGView" child when absent.
var view: CYGView:
	get:
		if _view != null and is_instance_valid(_view):
			return _view
		_view = _resolve_view()
		return _view


var view_id: int:
	get:
		var target := view
		return target.view_id if target != null else 0


## Actor number that owns the view, or -1 with no view.
var owner_actor: int:
	get:
		var target := view
		return target.owner_actor if target != null else -1


var is_mine: bool:
	get:
		var target := view
		return target != null and target.is_mine


var has_authority: bool:
	get:
		var target := view
		return target != null and target.has_authority


## True for editor-placed networked objects (owner actor 0).
var is_scene_object: bool:
	get:
		var target := view
		return target != null and target.is_scene_object


## This peer's player, or null before the join handshake completes.
var local_player: CYGPlayer:
	get:
		return CYGNetwork.local_player


## The player that owns the view; null for scene objects and unknown actors.
var owner_player: CYGPlayer:
	get:
		var target := view
		if target == null:
			return null
		return CYGNetwork.players.get(target.owner_actor) as CYGPlayer


func _ready() -> void:
	var target := view
	if target == null:
		push_warning("CYGActor3D %s: could not attach a CYGView; this node is not networked." % name)
		return
	# The view registers itself in its own `_ready`, which runs first for an
	# editor-placed child; this covers a view built while the actor was out of tree.
	CYGNetwork.register_view(target)
	if not target.spawned.is_connected(_handle_spawned):
		target.spawned.connect(_handle_spawned)
	if not target.despawned.is_connected(_handle_despawned):
		target.despawned.connect(_handle_despawned)
	if not target.owner_changed.is_connected(_handle_owner_changed):
		target.owner_changed.connect(_handle_owner_changed)
	_maybe_start_local()


## Called after the view spawns.
func cyg_on_spawned() -> void:
	pass


## Called before the view despawns.
func cyg_on_despawned() -> void:
	pass


## Called when the view's owner changes.
func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	pass


## Godot equivalent of Unity's `OnStartLocalPlayer`: once, when the view belongs
## to this peer.
func cyg_on_start_local() -> void:
	pass


## Sends an RPC bound on this actor's view. Named `send_rpc` because `Node.rpc`
## is the engine's own `@rpc` entry point and cannot be overridden from script.
func send_rpc(method_name: String, args: Array = [], target: int = CYGOpcodes.Target.ALL) -> bool:
	var network_view := view
	if network_view == null:
		push_error("CYGActor3D.send_rpc: no view for '%s'." % method_name)
		return false
	return network_view.send_rpc(method_name, args, target)


## Binds a handler for RPCs arriving on this actor's view.
func bind_rpc(method_name: String, handler: Callable) -> void:
	var network_view := view
	if network_view == null:
		push_error("CYGActor3D.bind_rpc: no view for '%s'." % method_name)
		return
	network_view.bind_rpc(method_name, handler)


func _handle_spawned() -> void:
	cyg_on_spawned()
	_maybe_start_local()


func _handle_despawned() -> void:
	cyg_on_despawned()


func _handle_owner_changed(new_owner: CYGPlayer) -> void:
	cyg_on_owner_changed(new_owner)
	_maybe_start_local()


func _maybe_start_local() -> void:
	if _start_local_fired:
		return
	var target := view
	if target != null and target.is_mine:
		_start_local_fired = true
		cyg_on_start_local()


func _resolve_view() -> CYGView:
	var existing := get_node_or_null("CYGView") as CYGView
	if existing == null:
		for child in get_children():
			if child is CYGView:
				existing = child
				break
	if existing != null:
		return existing
	var created := CYGView.new()
	created.name = "CYGView"
	add_child(created)
	return created
