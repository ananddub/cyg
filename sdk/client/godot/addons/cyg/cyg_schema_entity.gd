class_name CYGSchemaEntity
extends RefCounted
## One row of a schema collection, with Colyseus-style hooks.
##
## Field values live in `_fields` and are exposed as properties through
## `_get`/`_set`, so decoded state reads like a plain object (`player.x`).
## Entity identity survives full snapshots: the room updates this instance in
## place, which is what makes `listen()` callbacks and `onRemove` meaningful
## across a reconnect.

var id: int = 0
var _fields: Dictionary = {}
var _room: WeakRef = null


func _init(entity_id: int = 0, fields: Dictionary = {}, room: Variant = null) -> void:
	id = entity_id
	_fields = fields
	if room != null:
		_room = weakref(room)


func _get(property: StringName) -> Variant:
	var key := String(property)
	return _fields.get(key, null)


func _set(property: StringName, value: Variant) -> bool:
	var key := String(property)
	if key == "id":
		id = int(value)
		return true
	_fields[key] = value
	return true


func has_field(field_name: String) -> bool:
	return _fields.has(field_name)


func get_field(field_name: String, fallback: Variant = null) -> Variant:
	return _fields.get(field_name, fallback)


func set_field(field_name: String, value: Variant) -> void:
	_fields[field_name] = value


func field_names() -> Array:
	return _fields.keys()


func to_dict() -> Dictionary:
	var out := {"id": id}
	for key in _fields.keys():
		var value: Variant = _fields[key]
		out[key] = value.duplicate(true) if typeof(value) == TYPE_ARRAY else value
	return out


## Subscribe to one field: `handler.call(current, previous)`.
func listen(property: String, handler: Callable, immediate := true) -> Callable:
	var room: Variant = _room.get_ref() if _room != null else null
	if room == null:
		push_error("CYGSchemaEntity.listen: entity is not owned by a room")
		return Callable()
	return room._callbacks().listen(self, property, handler, immediate)


## Subscribe to every change on this entity: `handler.call()`.
func on_change(handler: Callable, immediate := false) -> Callable:
	var room: Variant = _room.get_ref() if _room != null else null
	if room == null:
		return Callable()
	return room._callbacks().on_change(self, handler, immediate)


## Mirror every field (or the listed ones) onto another object/dictionary.
func bind_to(target: Variant, properties: Array = [], immediate := true) -> Callable:
	var room: Variant = _room.get_ref() if _room != null else null
	if room == null:
		return Callable()
	return room._callbacks().bind_to(self, target, properties, immediate)


func _attach_room(room: Variant) -> void:
	if room != null:
		_room = weakref(room)
