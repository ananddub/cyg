class_name CYGPropertyMap
extends RefCounted
## Ordered, name-addressed sync properties for one `CYGBehaviour`.
##
## Delta block, byte-compatible with the Unity SDK's sync-var plumbing:
## `[dirtyCount u8]` then per dirty entry `[varIndex u8][value]`, entries being
## addressed by their registration index. Both sides must register the same
## names in the same order; a value is encoded by the type it was registered
## with, never by a per-value tag, so the block carries no type bytes.
##
## Unity's `OwnerAuth`/`SendInterval` throttles have no equivalent here: writes
## go through `CYGBehaviour.set_property()`, which drops them without authority.

const MAX_PROPERTIES := 255

var _names: PackedStringArray = PackedStringArray()
var _kinds: Array[int] = []
var _values: Array = []
var _dirty: Array[bool] = []
var _handlers: Array[Callable] = []


func size() -> int:
	return _names.size()


func index_of(name: String) -> int:
	return _names.find(name)


## Adds a tracked property and returns its wire index. `on_changed` is invoked as
## `on_changed(name, old, new)` whenever a remote delta rewrites the value.
func register(name: String, initial: Variant, on_changed: Callable = Callable()) -> int:
	var existing := index_of(name)
	if existing >= 0:
		push_warning("CYGPropertyMap: property '%s' is already registered at index %d." % [name, existing])
		return existing
	var kind := typeof(initial)
	if not _is_supported(kind):
		push_error("CYGPropertyMap: property '%s' has unsupported type %d." % [name, kind])
		return -1
	if _names.size() >= MAX_PROPERTIES:
		push_error("CYGPropertyMap: cannot register '%s'; %d properties is the wire limit." % [name, MAX_PROPERTIES])
		return -1
	_names.append(name)
	_kinds.append(kind)
	_values.append(initial)
	_dirty.append(false)
	_handlers.append(on_changed)
	return _names.size() - 1


func get_value(name: String) -> Variant:
	var index := index_of(name)
	return _values[index] if index >= 0 else null


## Writes a new value and marks it for the next delta. Returns false when the
## property is unknown or the value type differs from the registered one.
func set_value(name: String, value: Variant) -> bool:
	var index := index_of(name)
	if index < 0:
		push_error("CYGPropertyMap.set_value: unknown property '%s'." % name)
		return false
	if typeof(value) != _kinds[index]:
		push_error("CYGPropertyMap.set_value: '%s' expects type %d, got %d." % [name, _kinds[index], typeof(value)])
		return false
	if _values[index] == value:
		return true
	_values[index] = value
	_dirty[index] = true
	return true


## Forces every property into the next delta, e.g. for a joiner's snapshot.
func mark_all_dirty() -> void:
	for i in range(_dirty.size()):
		_dirty[i] = true


func has_dirty() -> bool:
	return _dirty.has(true)


func dirty_count() -> int:
	var count := 0
	for i in range(_dirty.size()):
		if _dirty[i]:
			count += 1
	return count


func serialize(writer: CYGBitPacker) -> void:
	writer.write_byte(dirty_count())
	for i in range(_names.size()):
		if not _dirty[i]:
			continue
		writer.write_byte(i)
		write_value(writer, _kinds[i], _values[i])
		_dirty[i] = false


func deserialize(reader: CYGBitReader) -> void:
	var count := reader.read_byte()
	for i in range(count):
		var index := reader.read_byte()
		if index >= _names.size():
			# An unknown-length value cannot be skipped: the rest of the payload
			# is undecodable, exactly as in the Unity reader.
			push_error("CYGPropertyMap: delta references property index %d, only %d registered." % [index, _names.size()])
			return
		var value := read_value(reader, _kinds[index])
		var previous: Variant = _values[index]
		_values[index] = value
		_dirty[index] = false
		if _handlers[index].is_valid():
			_handlers[index].call(_names[index], previous, value)


func values() -> Dictionary:
	var out := {}
	for i in range(_names.size()):
		out[_names[i]] = _values[i]
	return out


static func _is_supported(kind: int) -> bool:
	return kind in [
		TYPE_BOOL, TYPE_INT, TYPE_FLOAT, TYPE_STRING, TYPE_VECTOR2,
		TYPE_VECTOR3, TYPE_QUATERNION, TYPE_COLOR, TYPE_PACKED_BYTE_ARRAY,
	]


static func write_value(writer: CYGBitPacker, kind: int, value: Variant) -> void:
	match kind:
		TYPE_BOOL:
			writer.write_bool(value)
		TYPE_INT:
			writer.write_int64(value)
		TYPE_FLOAT:
			writer.write_single(value)
		TYPE_STRING:
			writer.write_string(value)
		TYPE_VECTOR2:
			writer.write_vector2(value)
		TYPE_VECTOR3:
			writer.write_vector3(value)
		TYPE_QUATERNION:
			writer.write_quaternion(value)
		TYPE_COLOR:
			writer.write_single(value.r)
			writer.write_single(value.g)
			writer.write_single(value.b)
			writer.write_single(value.a)
		TYPE_PACKED_BYTE_ARRAY:
			writer.write_bytes(value)


static func read_value(reader: CYGBitReader, kind: int) -> Variant:
	match kind:
		TYPE_BOOL:
			return reader.read_bool()
		TYPE_INT:
			return reader.read_int64()
		TYPE_FLOAT:
			return reader.read_single()
		TYPE_STRING:
			return reader.read_string()
		TYPE_VECTOR2:
			return reader.read_vector2()
		TYPE_VECTOR3:
			return reader.read_vector3()
		TYPE_QUATERNION:
			return reader.read_quaternion()
		TYPE_COLOR:
			return Color(reader.read_single(), reader.read_single(), reader.read_single(), reader.read_single())
		TYPE_PACKED_BYTE_ARRAY:
			return reader.read_bytes(reader.read_uint32())
		_:
			return null
