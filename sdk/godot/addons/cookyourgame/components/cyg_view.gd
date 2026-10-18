class_name CYGView
extends Node
## Network identity and ownership for a node, and the container for its
## observed (state-synced) components.
##
## Add it as a *child* node of the networked object; `network_node` resolves to
## the parent. Mirrors Unity's `CYGView`: view id layout is
## `(actor << 16) | counter`, owner actor 0 marks a scene object, and per-view
## deltas ride the wire as `[varint view_id][dirty mask byte][component blobs]`
## with blobs packed in ascending observed-component order.

signal spawned
signal despawned
signal owner_changed(new_owner: CYGPlayer)

const MAX_OBSERVED_COMPONENTS := 8

var view_id: int = 0
var owner_actor: int = -1

var observed_components: Array[Node] = []

## Set by a `CYGVisibilityCull` on this view when the object falls out of the
## observing room's interest range: the tick pump skips the view entirely, so a
## culled body costs no frames at all rather than an empty delta each tick.
var network_dormant: bool = false

var _auto_component := false


var network_node: Node:
	get:
		return get_parent()


var is_mine: bool:
	get:
		return CYGNetwork.local_player != null and owner_actor == CYGNetwork.local_player.actor_number


var is_scene_object: bool:
	get:
		return owner_actor == 0


var has_authority: bool:
	get:
		return is_mine or (is_scene_object and CYGNetwork.is_master_client)


func _ready() -> void:
	# A view placed in the editor has no id yet; the scene object convention is
	# actor 0, and the master client drives its state.
	if view_id == 0:
		_auto_component = true
		view_id = CYGNetwork.next_view_id_for_owner(0)
		owner_actor = 0
	CYGNetwork.register_view(self)


func _exit_tree() -> void:
	CYGNetwork.unregister_view(view_id)


func assign_view_id(new_view_id: int, new_owner_actor: int) -> void:
	if new_view_id <= 0:
		push_error("CYGView.assign_view_id: invalid view id %d on %s" % [new_view_id, network_node.name])
		return
	if view_id > 0 and view_id != new_view_id:
		CYGNetwork.unregister_view(view_id)
	view_id = new_view_id
	owner_actor = new_owner_actor
	CYGNetwork.register_view(self)


func add_observable(component: Node) -> void:
	if component == null or component in observed_components:
		return
	if observed_components.size() >= MAX_OBSERVED_COMPONENTS:
		push_warning("CYGView on %s: only %d observed components are supported (dirty bitmask limit)."
			% [network_node.name, MAX_OBSERVED_COMPONENTS])
		return
	observed_components.append(component)


func remove_observable(component: Node) -> void:
	observed_components.erase(component)


func invoke_spawn_hooks() -> void:
	for child in network_node.get_children():
		if child.has_method("cyg_on_spawned"):
			child.cyg_on_spawned()
	spawned.emit()


func invoke_despawn_hooks() -> void:
	for child in network_node.get_children():
		if child.has_method("cyg_on_despawned"):
			child.cyg_on_despawned()
	despawned.emit()
	CYGNetwork.unregister_view(view_id)


func invoke_owner_changed(new_owner: CYGPlayer) -> void:
	owner_actor = new_owner.actor_number if new_owner != null else -1
	for child in network_node.get_children():
		if child.has_method("cyg_on_owner_changed"):
			child.cyg_on_owner_changed(new_owner)
	owner_changed.emit(new_owner)


## True when at least one of the first 8 observed components reports a delta.
func has_any_delta() -> bool:
	var count := mini(observed_components.size(), MAX_OBSERVED_COMPONENTS)
	for i in range(count):
		var component := observed_components[i]
		if component != null and component.has_delta():
			return true
	return false


## Writes [dirty mask] then each dirty component's blob, ascending index order.
func collect_deltas(writer: CYGBitPacker) -> void:
	var count := mini(observed_components.size(), MAX_OBSERVED_COMPONENTS)
	var mask := 0
	for i in range(count):
		var component := observed_components[i]
		if component != null and component.has_delta():
			mask |= 1 << i
	writer.write_byte(mask)
	for i in range(count):
		if mask & (1 << i) == 0:
			continue
		observed_components[i].serialize(writer)


## Reads the dirty mask and feeds each blob to its component in the same order.
func apply_delta_stream(reader: CYGBitReader) -> void:
	var mask := reader.read_byte()
	for i in range(MAX_OBSERVED_COMPONENTS):
		if mask & (1 << i) == 0:
			continue
		if i >= observed_components.size():
			push_warning("CYGView %d: delta references component %d but only %d are attached"
				% [view_id, i, observed_components.size()])
			return
		var component := observed_components[i]
		if component != null:
			component.deserialize(reader)


## Binds a Callable as the handler for an incoming RPC name on this view.
## Godot has no attribute discovery, so this is the explicit equivalent of
## Unity's `[CYGRPC]` binding.
func bind_rpc(method_name: String, handler: Callable) -> void:
	CYGRpcRouter.bind(self, method_name, handler)


## Sends an RPC bound on this view (see `CYGRpcRouter`).
## Returns whether the frame reached the transport.
func send_rpc(method_name: String, args: Array = [], target: int = CYGOpcodes.Target.ALL) -> bool:
	return CYGRpcRouter.invoke(self, method_name, target, args)


## Sends an RPC to the room authority only (client -> server semantics).
func server_rpc(method_name: String, args: Array = []) -> bool:
	return CYGRpcRouter.invoke(self, method_name, CYGOpcodes.Target.MASTER_CLIENT, args)
