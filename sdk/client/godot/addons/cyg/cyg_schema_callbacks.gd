class_name CYGSchemaCallbacks
extends RefCounted
## Colyseus v5-style callback facade over one room (`getStateCallbacks(room)`).
##
## ```gdscript
## var callbacks := CYGCallbacks.get_state_callbacks(room)
## callbacks.on_add("players", func(player, id):
##     callbacks.listen(player, "x", func(current, previous): print(current))
## )
## ```

var room: Variant = null


func _init(target_room: Variant = null) -> void:
	room = target_room


func _collection_map(value: Variant) -> CYGSchemaMap:
	if typeof(value) != TYPE_STRING or room == null:
		return null
	var key := String(value)
	if room._collection_maps.has(key):
		return room._collection_maps[key]
	var root: Variant = room._root_collection
	if (root != null and key == String(root)) or (root == null and key == "players"):
		return room.state
	return null


## `listen(instance, property, handler)` or `listen("collection", handler)`.
func listen(target: Variant, property_or_handler: Variant, handler: Callable = Callable(),
		immediate := true) -> Callable:
	if typeof(target) == TYPE_STRING:
		var map := _collection_map(target)
		if map != null and typeof(property_or_handler) == TYPE_CALLABLE:
			return map.on_change(property_or_handler)
		# Not a collection: a root scalar, routed through the state entity.
		if room != null and room.state != null and typeof(property_or_handler) == TYPE_CALLABLE:
			return listen(room.state, target, property_or_handler, immediate)
		return Callable()
	if not (target is Object) or not handler.is_valid() or room == null:
		return Callable()
	var property := String(property_or_handler)
	room._track_entity(target)
	var meta: Dictionary = room._entity_callbacks.get(target, {})
	if meta.is_empty():
		return Callable()
	if not meta["fields"].has(property):
		meta["fields"][property] = []
	var list: Array = meta["fields"][property]
	list.append(handler)
	if immediate and not room._is_triggering:
		var current: Variant = _read_value(target, property)
		if current != null:
			handler.call(current, null)
	return func() -> void:
		var index := list.find(handler)
		if index >= 0:
			list.remove_at(index)


## Every-field change for one entity (or a collection's per-row change).
func on_change(target: Variant, handler: Callable, immediate := false) -> Callable:
	if typeof(target) == TYPE_STRING:
		var map := _collection_map(target)
		if map == null or not handler.is_valid():
			return Callable()
		return map.on_change(handler)
	if room == null or not (target is Object):
		return Callable()
	var meta: Dictionary = room._entity_callbacks.get(target, {})
	if meta.is_empty() or not handler.is_valid():
		return Callable()
	var list: Array = meta["any"]
	list.append(handler)
	if immediate:
		handler.call()
	return func() -> void:
		var index := list.find(handler)
		if index >= 0:
			list.remove_at(index)


func on_add(target: Variant, handler: Callable, immediate := true) -> Callable:
	var map := _collection_map(target)
	if map == null:
		return Callable()
	return map.on_add(handler, immediate)


func on_remove(target: Variant, handler: Callable) -> Callable:
	var map := _collection_map(target)
	if map == null:
		return Callable()
	return map.on_remove(handler)


## Mirror every property of `from` onto `to`, now and on every change.
func bind_to(from: Variant, to: Variant, properties: Array = [], immediate := true) -> Callable:
	if from == null or to == null or room == null:
		return Callable()
	var names := properties.duplicate()
	if names.is_empty():
		names = _own_keys(from)
	var copy_immediately: bool = immediate and not room._is_triggering
	var unsubscribers: Array[Callable] = []
	for name in names:
		var property := String(name)
		if typeof(_read_value(from, property)) == TYPE_CALLABLE:
			continue
		var current: Variant = _read_value(from, property)
		if copy_immediately and current != null:
			_write_value(to, property, current)
		var property_name := property
		unsubscribers.append(listen(from, property_name,
			func(value, _previous) -> void:
				_write_value(to, property_name, value)
		, false))
	return func() -> void:
		for unsubscribe in unsubscribers:
			unsubscribe.call()


static func _own_keys(source: Variant) -> Array:
	if typeof(source) == TYPE_DICTIONARY:
		return (source as Dictionary).keys()
	if source is CYGSchemaEntity:
		return (source as CYGSchemaEntity).field_names()
	var keys: Array = []
	for property in source.get_property_list():
		if int(property["usage"]) & PROPERTY_USAGE_SCRIPT_VARIABLE:
			keys.append(property["name"])
	return keys


static func _read_value(source: Variant, property: String) -> Variant:
	if typeof(source) == TYPE_DICTIONARY:
		return (source as Dictionary).get(property)
	if source is Object:
		return (source as Object).get(property)
	return null


static func _write_value(target: Variant, property: String, value: Variant) -> void:
	if typeof(target) == TYPE_DICTIONARY:
		(target as Dictionary)[property] = value
	elif target is Object:
		(target as Object).set(property, value)
