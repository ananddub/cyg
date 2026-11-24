class_name CYGCallbacks
extends RefCounted
## Namespace-level callback entry points, mirroring the JS `Callbacks` object.
##
## GDScript keeps `get` as an Object method, so the JS `Callbacks.get(room)`
## spelling is `CYGCallbacks.get_callbacks(room)` here; the v5 alias is
## `get_state_callbacks(room)`.

const VERSION := "1.5.0"


static func get_callbacks(room_or_decoder: Variant) -> CYGSchemaCallbacks:
	var target: Variant = room_or_decoder
	if typeof(room_or_decoder) == TYPE_DICTIONARY:
		target = (room_or_decoder as Dictionary).get("room", room_or_decoder)
	return CYGSchemaCallbacks.new(target)


## Alias matching Colyseus v5's `getStateCallbacks(room)`.
static func get_state_callbacks(room_or_decoder: Variant) -> CYGSchemaCallbacks:
	return get_callbacks(room_or_decoder)


static func get_raw_changes() -> Callable:
	return func() -> void:
		pass
