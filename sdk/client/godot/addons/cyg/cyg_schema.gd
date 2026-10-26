# CYG schema runtime for Godot 4.
#
# Decodes the `OXSD` descriptor and applies `OXS2` full/patch sections. The
# generated bindings (`cyg_schema_generated.gd`) subclass `Entity` and
# register typed factories with `Registry`:
#
#   var registry := CYGSchema.Registry.new()
#   CYGSchemaGenerated.register(registry)
#   var state := CYGSchema.State.create_from_descriptor(bytes, registry)
#   state.apply(section)
class_name CYGSchema
extends RefCounted

## Wire type codes shared by `OXSD` and `OXS2`.
enum WireKind {
	NONE = 0,
	U8 = 1,
	U16 = 2,
	U32 = 3,
	I32 = 4,
	F32 = 5,
	F64 = 6,
	BOOL = 7,
	I64 = 8,
	I8 = 9,
	I16 = 10,
	STRING = 11,
	ARRAY = 12,
}

const FLAG_OPTIONAL := 1
const FLAG_STATIC := 2
const FLAG_TRANSIENT := 4

static func _kind_size(kind: int) -> int:
	match kind:
		WireKind.U8, WireKind.I8, WireKind.BOOL:
			return 1
		WireKind.U16, WireKind.I16:
			return 2
		WireKind.U32, WireKind.I32, WireKind.F32:
			return 4
		WireKind.F64, WireKind.I64:
			return 8
		_:
			return 0

static func _kind_from_code(code: int) -> int:
	if code < 1 or code > 12:
		return WireKind.NONE
	return code


## One field of a schema, in wire order.
class FieldDef extends RefCounted:
	var name: String = ""
	var kind: int = WireKind.NONE
	var flags: int = 0
	var element: int = WireKind.NONE
	var size: int = 0

	func _init(field_name: String = "", field_kind: int = WireKind.NONE,
			field_flags: int = 0, field_element: int = WireKind.NONE) -> void:
		name = field_name
		kind = field_kind
		flags = field_flags
		element = field_element
		size = CYGSchema._kind_size(kind)

	func optional() -> bool:
		return (flags & CYGSchema.FLAG_OPTIONAL) != 0

	func full_only() -> bool:
		return (flags & (CYGSchema.FLAG_STATIC | CYGSchema.FLAG_TRANSIENT)) != 0


## A collection schema decoded from the host descriptor.
class SchemaDef extends RefCounted:
	var name: String = ""
	var version: int = 0
	var hash: int = 0
	var fields: Array = []

	func index_of(field_name: String) -> int:
		for i in fields.size():
			if fields[i].name == field_name:
				return i
		return -1


## A root map name and the schema it carries.
class CollectionDef extends RefCounted:
	var name: String = ""
	var schema: String = ""

	func _init(collection_name: String = "", schema_name: String = "") -> void:
		name = collection_name
		schema = schema_name


## The decoded `cyg.schema.bin` payload.
class RootDef extends RefCounted:
	var format_version: int = 0
	var name: String = ""
	var version: int = 0
	var hash: int = 0
	var scalars: Array = []
	var collections: Array = []
	var schemas: Array = []

	func find(schema_name: String) -> SchemaDef:
		for schema in schemas:
			if schema.name == schema_name:
				return schema
		return null


## Base state row. Generated classes override the accessors with typed fields.
class Entity extends RefCounted:
	var id: int = 0
	var definition: SchemaDef = null

	func field_count() -> int:
		return definition.fields.size() if definition != null else 0

	func set_field(_field: int, _value) -> void:
		pass

	func get_field(_field: int):
		return null

	func clear_fields() -> void:
		pass

	## Reads a field by name, or null when the schema has no such field.
	func get_value(field_name: String):
		var index := definition.index_of(field_name) if definition != null else -1
		return null if index < 0 else get_field(index)

	func has_field(field_name: String) -> bool:
		return definition != null and definition.index_of(field_name) >= 0


## Untyped row used when no generated class is registered.
class DynamicEntity extends Entity:
	var values: Array = []

	func _init(field_count: int = 0) -> void:
		values.resize(field_count)

	func set_field(field: int, value) -> void:
		if field >= 0 and field < values.size():
			values[field] = value

	func get_field(field: int):
		return values[field] if field >= 0 and field < values.size() else null

	func clear_fields() -> void:
		for i in values.size():
			values[i] = null


## Maps schema names to generated entity constructors.
class Registry extends RefCounted:
	var _factories: Dictionary = {}

	func register(schema_name: String, factory: Callable) -> void:
		if schema_name.is_empty() or not factory.is_valid():
			return
		_factories[schema_name] = factory

	func create(definition: SchemaDef) -> Entity:
		var entity: Entity = null
		if _factories.has(definition.name):
			var made = (_factories[definition.name] as Callable).call()
			if made is Entity:
				entity = made
		if entity == null:
			entity = DynamicEntity.new(definition.fields.size())
		entity.definition = definition
		entity.id = 0
		entity.clear_fields()
		return entity


## One root collection: entities keyed by their wire id.
class Collection extends RefCounted:
	var name: String = ""
	var definition: SchemaDef = null
	var entities: Dictionary = {}
	var order: Array = []
	var ready: bool = false

	func find(id: int) -> Entity:
		return entities.get(id, null)


## Little-endian, bounds-checked reader shared by both decoders.
class Reader extends RefCounted:
	var data: PackedByteArray
	var offset: int = 0
	var end: int = 0
	var failed: bool = false

	func _init(bytes: PackedByteArray, start: int = 0, stop: int = -1) -> void:
		data = bytes
		offset = start
		end = bytes.size() if stop < 0 else stop
		if offset < 0 or end > bytes.size() or offset > end:
			failed = true

	func need(count: int) -> bool:
		if failed:
			return false
		if count < 0 or offset + count > end:
			failed = true
			return false
		return true

	func u8() -> int:
		if not need(1):
			return 0
		var value := data.decode_u8(offset)
		offset += 1
		return value

	func i8() -> int:
		if not need(1):
			return 0
		var value := data.decode_s8(offset)
		offset += 1
		return value

	func u16() -> int:
		if not need(2):
			return 0
		var value := data.decode_u16(offset)
		offset += 2
		return value

	func i16() -> int:
		if not need(2):
			return 0
		var value := data.decode_s16(offset)
		offset += 2
		return value

	func u32() -> int:
		if not need(4):
			return 0
		var value := data.decode_u32(offset)
		offset += 4
		return value

	func i32() -> int:
		if not need(4):
			return 0
		var value := data.decode_s32(offset)
		offset += 4
		return value

	func i64() -> int:
		if not need(8):
			return 0
		var value := data.decode_s64(offset)
		offset += 8
		return value

	func f32() -> float:
		if not need(4):
			return 0.0
		var value := data.decode_float(offset)
		offset += 4
		return value

	func f64() -> float:
		if not need(8):
			return 0.0
		var value := data.decode_double(offset)
		offset += 8
		return value

	func string8() -> String:
		var length := u8()
		if failed or not need(length):
			return ""
		var value := data.slice(offset, offset + length).get_string_from_utf8()
		offset += length
		return value

	func string16() -> String:
		var length := u16()
		if failed or not need(length):
			return ""
		var value := data.slice(offset, offset + length).get_string_from_utf8()
		offset += length
		return value


## Decodes the `OXSD` descriptor sent by the host on join.
class Descriptor extends RefCounted:
	const VERSION := 3

	static func parse(bytes: PackedByteArray):
		if bytes.size() < 6:
			return null
		if bytes[0] != 79 or bytes[1] != 88 or bytes[2] != 83 or bytes[3] != 68:
			return null
		var reader := Reader.new(bytes, 4)
		var version := reader.u8()
		if reader.failed or version < 1 or version > VERSION:
			return null
		reader.u8() # flags

		var root := RootDef.new()
		root.format_version = version
		var schema_count := reader.u16()
		if reader.failed:
			return null
		for _i in range(schema_count):
			var schema := SchemaDef.new()
			schema.name = reader.string16()
			schema.version = reader.u32()
			schema.hash = reader.u32()
			var field_count := reader.u16()
			if reader.failed:
				return null
			for _f in range(field_count):
				var field = _read_field(reader, version)
				if field == null:
					return null
				schema.fields.append(field)
			root.schemas.append(schema)

		var root_name := reader.string16()
		if reader.failed:
			return null
		if version == 1:
			var index := reader.u16()
			if reader.failed:
				return null
			root.name = root_name
			if not root_name.is_empty() and index < root.schemas.size():
				root.collections.append(CollectionDef.new(root_name, root.schemas[index].name))
		else:
			root.version = reader.u32()
			root.hash = reader.u32()
			var scalar_count := reader.u16()
			if reader.failed:
				return null
			for _s in range(scalar_count):
				var scalar = _read_field(reader, version)
				if scalar == null:
					return null
				root.scalars.append(scalar)
			var collection_count := reader.u16()
			if reader.failed:
				return null
			for _c in range(collection_count):
				var collection_name := reader.string16()
				var index := reader.u16()
				if reader.failed or index >= root.schemas.size():
					return null
				root.collections.append(CollectionDef.new(collection_name, root.schemas[index].name))
			root.name = root_name

		if reader.offset != bytes.size():
			return null
		return root

	static func _read_field(reader: Reader, version: int):
		var name := reader.string16()
		var kind := CYGSchema._kind_from_code(reader.u8())
		if reader.failed or kind == WireKind.NONE:
			return null
		var flags := 0
		var element := WireKind.NONE
		if version >= 3:
			flags = reader.u8()
			element = CYGSchema._kind_from_code(reader.u8())
			if reader.failed:
				return null
		return FieldDef.new(name, kind, flags, element)


## Applies one `OXS2` section (full snapshot or field patch) to the state.
class State extends RefCounted:
	signal entity_added(collection: String, id: int, entity)
	signal entity_removed(collection: String, id: int, entity)
	signal entity_updated(collection: String, id: int, entity)
	signal field_changed(collection: String, id: int, field: int, value, previous)
	signal root_field_changed(field: int, value, previous)

	const MAGIC := [79, 88, 83, 50] # "OXS2"

	var root: RootDef = null
	var collections: Array = []
	var root_entity: Entity = null
	var ready: bool = false
	var valid: bool = false

	var _registry: Registry = null
	var _root_definition: SchemaDef = null

	func _init(root_def: RootDef = null, registry: Registry = null) -> void:
		if root_def == null:
			return
		root = root_def
		_registry = registry if registry != null else Registry.new()
		for def in root.collections:
			var schema := root.find(def.schema)
			if schema == null:
				push_error("CYG schema: unknown schema '%s'" % def.schema)
				return
			var collection := Collection.new()
			collection.name = def.name
			collection.definition = schema
			collections.append(collection)

		_root_definition = SchemaDef.new()
		_root_definition.name = root.name
		_root_definition.version = root.version
		_root_definition.hash = root.hash
		_root_definition.fields = root.scalars
		root_entity = _registry.create(_root_definition)
		valid = true

	## Parse an `OXSD` descriptor and build its state.
	static func create_from_descriptor(bytes: PackedByteArray, registry: Registry = null):
		var root_def = Descriptor.parse(bytes)
		if root_def == null:
			return null
		var state = State.new(root_def, registry)
		return state if state.valid else null

	static func create(bytes: PackedByteArray, registry: Registry = null):
		return create_from_descriptor(bytes, registry)

	func collection(name: String) -> Collection:
		for entry in collections:
			if entry.name == name:
				return entry
		return null

	## Apply one section. A malformed or unknown frame is ignored.
	func apply(section: PackedByteArray) -> bool:
		var reader := Reader.new(section)
		if not _is_magic(reader):
			return false
		var kind := reader.u8()
		if reader.failed:
			return false
		if kind == 1 or kind == 2:
			if collections.is_empty():
				return false
			return _apply_body(kind, collections[0], reader)
		if kind == 3 or kind == 4:
			return _apply_world(kind, reader)
		return false

	func _is_magic(reader: Reader) -> bool:
		for byte in MAGIC:
			if reader.u8() != byte:
				return false
		return not reader.failed

	func _apply_body(kind: int, collection: Collection, reader: Reader) -> bool:
		return _apply_full(collection, reader) if kind == 1 else _apply_patch(collection, reader)

	func _apply_section(collection: Collection, reader: Reader, expected: int) -> bool:
		if not _is_magic(reader):
			return false
		var kind := reader.u8()
		if reader.failed or kind != expected:
			return false
		return _apply_body(kind, collection, reader)

	func _apply_full(collection: Collection, reader: Reader) -> bool:
		var hash := reader.u32()
		if reader.failed or hash != collection.definition.hash:
			return false
		var count := reader.u32()
		if reader.failed:
			return false

		var fields: Array = collection.definition.fields
		var incoming: Array = []
		var ids: Dictionary = {}
		for _i in range(count):
			var id := reader.u32()
			if reader.failed:
				return false
			if ids.has(id):
				return false
			ids[id] = true
			var values: Array = []
			for field in fields:
				values.append(_read_value(reader, field))
				if reader.failed:
					return false
			incoming.append([id, values])

		for i in range(collection.order.size() - 1, -1, -1):
			var id: int = collection.order[i]
			if ids.has(id):
				continue
			var entity: Entity = collection.entities.get(id, null)
			if entity == null:
				continue
			entity_removed.emit(collection.name, id, entity)
			collection.entities.erase(id)
			collection.order.remove_at(i)

		for entry in incoming:
			var id: int = entry[0]
			var values: Array = entry[1]
			var entity: Entity = collection.entities.get(id, null)
			if entity != null:
				_apply_values(collection, id, entity, values, true)
				continue
			entity = _registry.create(collection.definition)
			entity.id = id
			_apply_values(collection, id, entity, values, false)
			collection.entities[id] = entity
			collection.order.append(id)
			entity_added.emit(collection.name, id, entity)
		collection.ready = true
		return true

	func _apply_patch(collection: Collection, reader: Reader) -> bool:
		var hash := reader.u32()
		if reader.failed or hash != collection.definition.hash:
			return false
		# A patch before the full row would be lost; the host keeps sending
		# full snapshots until the client acknowledges the baseline.
		if not collection.ready:
			return false
		var operations := reader.u32()
		if reader.failed:
			return false

		var fields: Array = collection.definition.fields
		for _i in range(operations):
			var op := reader.u8()
			if reader.failed:
				return false
			if op == 1:
				var id := reader.u32()
				if reader.failed:
					return false
				var values: Array = []
				for field in fields:
					values.append(_read_value(reader, field))
					if reader.failed:
						return false
				var entity: Entity = collection.entities.get(id, null)
				if entity != null:
					_apply_values(collection, id, entity, values, true)
				else:
					entity = _registry.create(collection.definition)
					entity.id = id
					_apply_values(collection, id, entity, values, false)
					collection.entities[id] = entity
					collection.order.append(id)
					entity_added.emit(collection.name, id, entity)
			elif op == 2:
				var id := reader.u32()
				if reader.failed:
					return false
				var entity: Entity = collection.entities.get(id, null)
				if entity != null:
					entity_removed.emit(collection.name, id, entity)
					collection.entities.erase(id)
					collection.order.erase(id)
			elif op == 3:
				var id := reader.u32()
				var mask := reader.u32()
				if reader.failed:
					return false
				var entity: Entity = collection.entities.get(id, null)
				var changed := false
				for field_index in fields.size():
					if field_index >= 32 or (mask & (1 << field_index)) == 0:
						continue
					var value = _read_value(reader, fields[field_index])
					if reader.failed:
						return false
					if entity == null:
						continue
					var previous = entity.get_field(field_index)
					entity.set_field(field_index, value)
					if previous == value:
						continue
					changed = true
					field_changed.emit(collection.name, id, field_index, value, previous)
				if changed and entity != null:
					entity_updated.emit(collection.name, id, entity)
			else:
				return false
		return true

	func _apply_world(kind: int, reader: Reader) -> bool:
		# A world patch before the baseline would be lost; the host repeats
		# full frames until the client acknowledges.
		if kind == 4 and not ready:
			return false

		var root_hash := reader.u32()
		var root_count := reader.u16()
		if reader.failed:
			return false
		if root_count != root.scalars.size() or root_hash != _root_definition.hash:
			return false

		var mask := 0
		if kind == 4:
			mask = reader.u32()
			if reader.failed:
				return false
		for field_index in root.scalars.size():
			if kind == 4 and (field_index >= 32 or (mask & (1 << field_index)) == 0):
				continue
			var value = _read_value(reader, root.scalars[field_index])
			if reader.failed:
				return false
			var previous = root_entity.get_field(field_index)
			root_entity.set_field(field_index, value)
			if previous != value:
				root_field_changed.emit(field_index, value, previous)

		var count := reader.u16()
		if reader.failed:
			return false
		for _i in range(count):
			var collection_id := reader.u16()
			var hash := reader.u32()
			var length := reader.u32()
			if reader.failed:
				return false
			if collection_id >= collections.size():
				return false
			var collection: Collection = collections[collection_id]
			if hash != collection.definition.hash:
				return false
			if not reader.need(length):
				return false
			var section := Reader.new(reader.data.slice(reader.offset, reader.offset + length))
			var expected := 1 if kind == 3 else 2
			if not _apply_section(collection, section, expected):
				return false
			if section.offset != length:
				return false
			reader.offset += length
		if reader.offset != reader.end:
			return false
		if kind == 3:
			ready = true
		return true

	func _apply_values(collection: Collection, id: int, entity: Entity, values: Array,
			fire: bool) -> void:
		var changed := false
		for field_index in values.size():
			var previous = entity.get_field(field_index)
			var value = values[field_index]
			entity.set_field(field_index, value)
			if previous == value:
				continue
			changed = true
			if fire:
				field_changed.emit(collection.name, id, field_index, value, previous)
		if changed and fire:
			entity_updated.emit(collection.name, id, entity)

	static func _read_value(reader: Reader, field: FieldDef):
		if field.optional():
			var present := reader.u8()
			if reader.failed:
				return null
			if present == 0:
				return null
		if field.kind != WireKind.ARRAY:
			return _read_scalar(reader, field.kind)

		var element: int = field.element
		if element == WireKind.NONE:
			element = WireKind.F64
		var count := reader.u8()
		if reader.failed:
			return null
		var values: Array = []
		values.resize(count)
		for i in range(count):
			values[i] = _read_scalar(reader, element)
			if reader.failed:
				return null
		return values

	static func _read_scalar(reader: Reader, kind: int):
		match kind:
			WireKind.U8:
				return reader.u8()
			WireKind.I8:
				return reader.i8()
			WireKind.U16:
				return reader.u16()
			WireKind.I16:
				return reader.i16()
			WireKind.U32:
				return reader.u32()
			WireKind.I32:
				return reader.i32()
			WireKind.F32:
				return reader.f32()
			WireKind.F64:
				return reader.f64()
			WireKind.BOOL:
				return reader.u8() != 0
			WireKind.I64:
				return reader.i64()
			WireKind.STRING:
				return reader.string8()
			_:
				reader.failed = true
				return null
