class_name OpenOxideCallbacks
extends RefCounted
## Namespace-level callback entry points, mirroring the JS `Callbacks` object.
##
## GDScript keeps `get` as an Object method, so the JS `Callbacks.get(room)`
## spelling is `OpenOxideCallbacks.get_callbacks(room)` here. The other two
## names keep their JS shape: `get_state_callbacks(room)` and `get_legacy(room)`.

const VERSION := "1.5.0"


static func get_callbacks(room_or_decoder: Variant) -> OpenOxideSchemaCallbacks:
	var target: Variant = room_or_decoder
	if typeof(room_or_decoder) == TYPE_DICTIONARY:
		target = (room_or_decoder as Dictionary).get("room", room_or_decoder)
	return OpenOxideSchemaCallbacks.new(target)


## Alias matching Colyseus v5's `getStateCallbacks(room)`.
static func get_state_callbacks(room_or_decoder: Variant) -> OpenOxideSchemaCallbacks:
	return get_callbacks(room_or_decoder)


## Legacy `$(room.state).players.onAdd(...)` shape. The returned map already
## routes collection names and root scalars through `_get`, so it behaves like
## the JS proxy without needing one.
static func get_legacy(room: Variant) -> Variant:
	if room == null:
		return null
	return room.state if room.state != null else room


static func get_raw_changes() -> Callable:
	return func() -> void:
		pass
