class_name OpenOxideSchemaMap
extends RefCounted
## One schema collection (`room.state`, or `room.state.<name>` for the second
## and later collections): an id -> entity map with Colyseus-style hooks.
##
## `_get` also exposes sibling collections and root scalars, so the JS idiom
## `room.state.players` and `room.state.tick` keeps working from GDScript
## (the first collection returns itself).

var _room: WeakRef = null
var _name := ""
var _schema: OpenOxideSchemaDef = null
var _entries: Dictionary = {}  # id -> OpenOxideSchemaEntity
var _ready := false

var _on_add_handlers: Array[Callable] = []
var _on_remove_handlers: Array[Callable] = []
var _on_change_handlers: Array[Callable] = []


func _init(room: Variant = null, collection_name := "", schema: OpenOxideSchemaDef = null) -> void:
	if room != null:
		_room = weakref(room)
	_name = collection_name
	_schema = schema


func _get(property: StringName) -> Variant:
	var room: Variant = _room.get_ref() if _room != null else null
	if room == null:
		return null
	var key := String(property)
	if room._collection_maps.has(key):
		return room._collection_maps[key]
	if room._root_values.has(key):
		return room._root_values[key]
	return null


func get_name() -> String:
	return _name


func get_schema() -> OpenOxideSchemaDef:
	return _schema


func is_ready() -> bool:
	return _ready


func size() -> int:
	return _entries.size()


func is_empty() -> bool:
	return _entries.is_empty()


func has_id(entity_id: int) -> bool:
	return _entries.has(entity_id)


func get_entity(entity_id: int) -> OpenOxideSchemaEntity:
	return _entries.get(entity_id)


func set_entity(entity_id: int, entity: OpenOxideSchemaEntity) -> void:
	_entries[entity_id] = entity


func erase_entity(entity_id: int) -> void:
	_entries.erase(entity_id)


func ids() -> Array:
	return _entries.keys()


func values() -> Array:
	return _entries.values()


## Dictionary view, Colyseus `toJSON()` shape (ids become string keys).
func to_dict() -> Dictionary:
	var out := {}
	for entity_id in _entries.keys():
		out[str(entity_id)] = (_entries[entity_id] as OpenOxideSchemaEntity).to_dict()
	return out


func to_json() -> String:
	return JSON.stringify(to_dict())


func on_add(handler: Callable, immediate := true) -> Callable:
	if not handler.is_valid():
		return Callable()
	_on_add_handlers.append(handler)
	var room: Variant = _room.get_ref() if _room != null else null
	if immediate and (room == null or not room._is_triggering):
		for entity_id in _entries.keys():
			handler.call(_entries[entity_id], entity_id)
	return _unsubscribe(_on_add_handlers, handler)


func on_remove(handler: Callable) -> Callable:
	if not handler.is_valid():
		return Callable()
	_on_remove_handlers.append(handler)
	return _unsubscribe(_on_remove_handlers, handler)


func on_change(handler: Callable) -> Callable:
	if not handler.is_valid():
		return Callable()
	_on_change_handlers.append(handler)
	return _unsubscribe(_on_change_handlers, handler)


func _unsubscribe(list: Array[Callable], handler: Callable) -> Callable:
	return func() -> void:
		var index := list.find(handler)
		if index >= 0:
			list.remove_at(index)
