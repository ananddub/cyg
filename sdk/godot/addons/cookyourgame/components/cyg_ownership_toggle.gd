class_name CYGOwnershipToggle
extends CYGBehaviour
## Purely-local ownership switch: turns the "owner" set on when the view belongs
## to this peer and the "proxy" set on otherwise.
##
## It sends nothing and costs zero wire bytes. It derives from `CYGBehaviour`
## only to inherit the spawn/owner-change hooks the view walks; it never calls
## `register_as_observable()`, so it never enters the dirty mask, and
## `has_delta()` is false.
##
## Godot has no `Behaviour.enabled`, so `apply()` drives the most faithful
## equivalent per node type (see `_set_node_enabled`).

## Nodes enabled only for the local owner: input, camera, local effects.
## `Array[NodePath]` rather than `Array[Node]`: a scene file stores node
## references as paths, and Godot cannot cast those back into a typed Node array.
@export var enable_when_owner: Array[NodePath] = []
## Nodes enabled only for non-owners: remote-only nodes such as hitboxes.
@export var disable_when_owner: Array[NodePath] = []

var _warned_no_view := false


func _ready() -> void:
	apply()


func cyg_on_spawned() -> void:
	apply()


func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	apply()


## Re-evaluates both lists against the current ownership; safe to call any time.
func apply() -> void:
	var target := view
	if target == null:
		if not _warned_no_view:
			_warned_no_view = true
			push_warning("CYGOwnershipToggle on %s has no CYGView in its parent chain; ownership toggle is a no-op."
				% name)
		return
	var mine: bool = target.is_mine
	_set_group(enable_when_owner, mine)
	_set_group(disable_when_owner, not mine)


func _set_group(nodes: Array[NodePath], enabled: bool) -> void:
	for node_path in nodes:
		_set_node_enabled(get_node_or_null(node_path), enabled)


func _set_node_enabled(node: Node, enabled: bool) -> void:
	if node == null or not is_instance_valid(node):
		return
	if node is CollisionObject3D or node is CollisionObject2D \
			or node is CollisionShape3D or node is CollisionShape2D:
		_toggle_collision(node, enabled)
	elif node is CanvasItem:
		(node as CanvasItem).visible = enabled
	elif node is VisualInstance3D:
		(node as VisualInstance3D).visible = enabled
	# Both exist on every Node, so this always applies.
	node.set_process(enabled)
	node.set_physics_process(enabled)


## Collision-side switch. `disabled` is deferred (physics state must not change
## inside a notification) and, in Godot 4, lives on `CollisionShape2D`/
## `CollisionShape3D` rather than on the `CollisionObject*` base, so a body is
## disabled through its own shapes and areas through their monitoring flags.
func _toggle_collision(node: Node, enabled: bool) -> void:
	if _has_property(node, "disabled"):
		node.set_deferred("disabled", not enabled)
	if node is Area2D:
		(node as Area2D).monitoring = enabled
		(node as Area2D).monitorable = enabled
	elif node is Area3D:
		(node as Area3D).monitoring = enabled
		(node as Area3D).monitorable = enabled
	for child in node.get_children():
		if _has_property(child, "disabled"):
			child.set_deferred("disabled", not enabled)


func _has_property(node: Node, property_name: String) -> bool:
	for property in node.get_property_list():
		if str(property["name"]) == property_name:
			return true
	return false


func has_delta() -> bool:
	return false


## No wire state. Deliberately does not call `super`: this component registers no
## properties, so writing nothing must stay byte-symmetric with reading nothing.
func serialize(_writer: CYGBitPacker) -> void:
	pass


func deserialize(_reader: CYGBitReader) -> void:
	pass
