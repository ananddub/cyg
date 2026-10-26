class_name CYGBehaviour
extends Node
## Base class for networked components that live beside a `CYGView`.
##
## Provides the ownership queries, the spawn/despawn/owner-change hooks and the
## sync-property plumbing every observed component needs. A component registers
## itself with `register_as_observable()` and its view walks it through the
## dirty mask; overrides of `serialize`/`deserialize` must call `super` first so
## the property block stays in front of the component's own fields.
##
## Godot has no attribute discovery, so the Unity `[CYGRPC]` equivalents are
## explicit: bind handlers with `CYGView.bind_rpc()` and drive per-tick work from
## the `CYGNetwork.network_tick` signal.

var _view: CYGView = null
var _properties: CYGPropertyMap = null
var _is_observable := false


## The view this component syncs through; resolved from the owning node once.
var view: CYGView:
	get:
		if _view != null and is_instance_valid(_view):
			return _view
		_view = _resolve_view()
		return _view


var has_authority: bool:
	get:
		var target := view
		return target != null and target.has_authority


var is_mine: bool:
	get:
		var target := view
		return target != null and target.is_mine


## Sync properties of this component; created on the first `register_property`.
var properties: CYGPropertyMap:
	get:
		if _properties == null:
			_properties = CYGPropertyMap.new()
		return _properties


## Called after the view spawns.
func cyg_on_spawned() -> void:
	pass


## Called before the view despawns.
func cyg_on_despawned() -> void:
	pass


## Called when the view's owner changes.
func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	pass


## Declares a tracked property. `on_changed` runs as `on_changed(name, old, new)`
## on remote applies; registration order is the wire identity, so it must be
## identical on every peer.
func register_property(name: String, initial: Variant, on_changed: Callable = Callable()) -> int:
	var index := properties.register(name, initial, on_changed)
	if index >= 0:
		register_as_observable()
	return index


## Authority-side write; ignored when this peer does not own the view.
func set_property(name: String, value: Variant) -> bool:
	if not has_authority:
		push_warning("CYGBehaviour: '%s' dropped a write to '%s' without authority." % [name, self])
		return false
	return properties.set_value(name, value)


func get_property(name: String) -> Variant:
	return properties.get_value(name) if _properties != null else null


func register_as_observable() -> void:
	if _is_observable:
		return
	var target := view
	if target == null:
		push_warning("CYGBehaviour %s: no CYGView on the parent node; state will not sync." % name)
		return
	target.add_observable(self)
	_is_observable = true


func unregister_as_observable() -> void:
	if not _is_observable:
		return
	if _view != null and is_instance_valid(_view):
		_view.remove_observable(self)
	_is_observable = false


func _exit_tree() -> void:
	unregister_as_observable()


## True when the component has state to send. Subclasses add their own condition
## to this one.
func has_delta() -> bool:
	return _properties != null and _properties.has_dirty()


## Writes the property block; a component that never registered properties
## writes nothing, and its `deserialize` must not read either.
func serialize(writer: CYGBitPacker) -> void:
	if _properties != null and _properties.size() > 0:
		_properties.serialize(writer)


func deserialize(reader: CYGBitReader) -> void:
	if _properties != null and _properties.size() > 0:
		_properties.deserialize(reader)


## Sends an RPC on this component's view. Named `send_rpc` because `Node.rpc` is
## the engine's own `@rpc` entry point and cannot be overridden from script.
func send_rpc(method_name: String, args: Array = [], target: int = CYGOpcodes.Target.ALL) -> bool:
	var target_view := view
	if target_view == null:
		push_error("CYGBehaviour.send_rpc: no view for '%s'." % method_name)
		return false
	return target_view.send_rpc(method_name, args, target)


## Binds a handler for RPCs arriving on this component's view.
func bind_rpc(method_name: String, handler: Callable) -> void:
	var target_view := view
	if target_view == null:
		push_error("CYGBehaviour.bind_rpc: no view for '%s'." % method_name)
		return
	target_view.bind_rpc(method_name, handler)


func _resolve_view() -> CYGView:
	var parent := get_parent()
	if parent == null:
		return null
	if parent is CYGView:
		return parent
	for child in parent.get_children():
		if child is CYGView:
			return child
	# A component may sit deep inside the networked node (a socket publisher under a
	# `BoneAttachment3D`, a hitbox inside a collider), where no sibling carries the
	# view. Walking up finds the nearest ancestor that owns one.
	var ancestor := parent.get_parent()
	while ancestor != null:
		if ancestor is CYGView:
			return ancestor
		for child in ancestor.get_children():
			if child is CYGView:
				return child
		ancestor = ancestor.get_parent()
	return null
