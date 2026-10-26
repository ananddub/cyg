class_name CYGSchemaDef
extends RefCounted
## Fluent schema definition and "OXSD" descriptor parser — the GDScript
## counterpart of `SchemaDef` / `parseSchemaDescriptor` in
## `sdk/web/cyg.js`.
##
## ```gdscript
## var schema := CYGSchemaDef.new("Player", 1).f64("x").f64("y").u32("flags")
## room.use_schema(schema)  # optional; a live room attaches the descriptor itself
## ```
##
## The hash is the exact FNV-1a the Rust host and the JS SDK compute, so a
## schema built here can be checked against `hash == 0x44d746fc` for
## `Player/1 + f64 x + f64 y + u32 flags`.

const MAX_FIELDS := 32
const FLAG_OPTIONAL := 1
const FLAG_STATIC := 2
const FLAG_TRANSIENT := 4

var name: String = ""
var version: int = 1
var fields: Array = []
var hash: int = 0
## Last builder/parse error; empty when the definition is valid.
var last_error := ""

var _by_name: Dictionary = {}


func _init(schema_name: String = "", schema_version: int = 1) -> void:
	name = schema_name
	version = schema_version & 0xFFFFFFFF
	hash = CYGCodec.fnv1a(2166136261, "%s|%d" % [name, version])


func _add(field_name: String, type_name: String, flags: int = 0, element: String = "") -> CYGSchemaDef:
	if fields.size() >= MAX_FIELDS:
		return self
	if _by_name.has(field_name):
		last_error = "duplicate schema field '%s'" % field_name
		return self
	var code: int = CYGCodec.SCHEMA_TYPE_CODE.get(type_name, 0)
	if code == 0:
		last_error = "unknown schema field type '%s'" % type_name
		return self
	var element_code := 0
	if not element.is_empty():
		element_code = CYGCodec.SCHEMA_TYPE_CODE.get(element, 0)
		if element_code == 0:
			last_error = "unknown array element type '%s'" % element
			return self
	var offset := 0
	for existing in fields:
		offset += int(existing["size"])
	fields.append({
		"name": field_name,
		"type": type_name,
		"code": code,
		"flags": flags,
		"optional": (flags & FLAG_OPTIONAL) != 0,
		"full_only": (flags & (FLAG_STATIC | FLAG_TRANSIENT)) != 0,
		"element": "" if element.is_empty() else element,
		"element_code": element_code,
		"size": int(CYGCodec.SCHEMA_FIELD_SIZE[type_name]),
		"offset": offset,
	})
	_by_name[field_name] = fields.back()
	_rehash()
	last_error = ""
	return self


func _rehash() -> void:
	var value := CYGCodec.fnv1a(2166136261, "%s|%d" % [name, version])
	for field in fields:
		var text := "|%s:%d" % [field["name"], field["code"]]
		if int(field["element_code"]) != 0:
			text += ".%d" % int(field["element_code"])
		if int(field["flags"]) != 0:
			text += ":%d" % int(field["flags"])
		value = CYGCodec.fnv1a(value, text)
	hash = value


func _last() -> Dictionary:
	return fields.back() if not fields.is_empty() else {}


func optional() -> CYGSchemaDef:
	var field: Dictionary = _last()
	if not field.is_empty():
		field["flags"] = int(field["flags"]) | FLAG_OPTIONAL
		field["optional"] = true
		_rehash()
	return self


## JS `.static()`; `static` is a GDScript keyword, so the name is spelled out.
func static_field() -> CYGSchemaDef:
	var field: Dictionary = _last()
	if not field.is_empty():
		field["flags"] = int(field["flags"]) | FLAG_STATIC
		field["full_only"] = true
		_rehash()
	return self


## Alias matching the JS `staticField()` spelling.
func staticField() -> CYGSchemaDef:
	return static_field()


func transient() -> CYGSchemaDef:
	var field: Dictionary = _last()
	if not field.is_empty():
		field["flags"] = int(field["flags"]) | FLAG_TRANSIENT
		field["full_only"] = true
		_rehash()
	return self


func u8(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "u8")


func i8(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "i8")


func u16(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "u16")


func i16(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "i16")


func u32(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "u32")


func i32(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "i32")


func f32(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "f32")


func f64(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "f64")


func bool(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "bool")


func i64(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "i64")


func string(field_name: String) -> CYGSchemaDef:
	return _add(field_name, "string")


func array(field_name: String, element_type: String) -> CYGSchemaDef:
	return _add(field_name, "array", 0, element_type)


func index_of(field_name: String) -> int:
	var field: Variant = _by_name.get(field_name)
	return fields.find(field) if field != null else -1


func get_field(field_name: String) -> Dictionary:
	return _by_name.get(field_name, {})


## ------------------------------------------------------------ descriptor

static func is_descriptor(bytes: PackedByteArray) -> bool:
	if bytes.size() < 8:
		return false
	for i in range(4):
		if int(bytes[i]) != int(CYGCodec.DESCRIPTOR_MAGIC[i]):
			return false
	return true


## Parse an "OXSD" frame. v3 adds field flags and array element types, v2 adds
## root scalars and multiple collections, v1 is still accepted.
##
## Returns `{ version, schemas, root_name, root_scalars, collections, root }`
## or `{}` when the bytes are not a valid descriptor.
static func parse_descriptor(bytes: PackedByteArray) -> Dictionary:
	if not is_descriptor(bytes):
		return {}
	var reader := CYGCodec.Reader.new(bytes)
	reader.offset = 4
	var schema_version := reader.read_u8()
	if schema_version < 1 or schema_version > CYGCodec.DESCRIPTOR_VERSION:
		return {}
	reader.read_u8()  # flags
	var schema_count := reader.read_u16()
	var schemas: Array = []
	for i in range(schema_count):
		if not reader.ok:
			return {}
		var schema_name := reader.read_string16()
		var item_version := reader.read_u32()
		reader.read_u32()  # hash: recomputed from the fields below
		var field_count := reader.read_u16()
		var definition := CYGSchemaDef.new(schema_name, item_version)
		for f in range(field_count):
			if not _read_descriptor_field(reader, definition, schema_version):
				return {}
		schemas.append(definition)

	var root_name := reader.read_string16()
	var collections: Array = []
	var root_scalars: Variant = null

	if schema_version == 1:
		var root_index := reader.read_u16()
		var root_schema: Variant = null
		if root_index != 0xFFFF and root_index < schemas.size():
			root_schema = schemas[root_index]
		if root_schema != null and not root_name.is_empty():
			collections.append({"name": root_name, "schema": root_schema})
	else:
		var root_version := reader.read_u32()
		reader.read_u32()  # root hash: recomputed from the fields below
		var scalar_count := reader.read_u16()
		if scalar_count > 0:
			root_scalars = CYGSchemaDef.new(root_name if not root_name.is_empty() else "State", root_version)
			for f in range(scalar_count):
				if not _read_descriptor_field(reader, root_scalars, schema_version):
					return {}
		var collection_count := reader.read_u16()
		for i in range(collection_count):
			if not reader.ok:
				return {}
			var collection_name := reader.read_string16()
			var index := reader.read_u16()
			if collection_name.is_empty() or index == 0xFFFF or index >= schemas.size():
				return {}
			collections.append({"name": collection_name, "schema": schemas[index]})

	if not reader.ok or reader.offset != bytes.size():
		return {}
	var root: Variant = null
	if not collections.is_empty():
		root = {"name": collections[0]["name"], "schema": collections[0]["schema"]}
	return {
		"version": schema_version,
		"schemas": schemas,
		"root_name": root_name,
		"root_scalars": root_scalars,
		"collections": collections,
		"root": root,
	}


static func _read_descriptor_field(reader: CYGCodec.Reader,
		definition: CYGSchemaDef, descriptor_version: int) -> bool:
	var field_name := reader.read_string16()
	var type_code := reader.read_u8()
	if type_code <= 0 or type_code >= CYGCodec.SCHEMA_TYPE_NAME.size():
		return false
	var type_name: String = CYGCodec.SCHEMA_TYPE_NAME[type_code]
	if type_name.is_empty():
		return false
	var flags := 0
	var element := ""
	if descriptor_version >= 3:
		flags = reader.read_u8()
		var element_code := reader.read_u8()
		if element_code != 0:
			if element_code >= CYGCodec.SCHEMA_TYPE_NAME.size():
				return false
			element = CYGCodec.SCHEMA_TYPE_NAME[element_code]
			if element.is_empty():
				return false
	definition._add(field_name, type_name, flags, element)
	return reader.ok and definition.last_error.is_empty()


## ----------------------------------------------------------- test helper

## Minimal v2 descriptor builder (root scalars + a single collection), used by
## the offline test and by games that ship a descriptor rather than a schema.
static func build_descriptor_v2(root_name: String, root_scalars: CYGSchemaDef,
		collections: Array) -> PackedByteArray:
	var out := PackedByteArray()
	for byte in CYGCodec.DESCRIPTOR_MAGIC:
		out.append(byte)
	out.append(2)   # version
	out.append(0)   # flags
	_put_u16(out, collections.size())
	for entry in collections:
		var schema: CYGSchemaDef = entry["schema"]
		_put_string16(out, schema.name)
		_put_u32(out, schema.version)
		_put_u32(out, schema.hash)
		_put_u16(out, schema.fields.size())
		for field in schema.fields:
			_put_string16(out, field["name"])
			out.append(int(field["code"]))
	_put_string16(out, root_name)
	_put_u32(out, root_scalars.version if root_scalars != null else 1)
	_put_u32(out, root_scalars.hash if root_scalars != null else 0)
	var scalar_count := root_scalars.fields.size() if root_scalars != null else 0
	_put_u16(out, scalar_count)
	if root_scalars != null:
		for field in root_scalars.fields:
			_put_string16(out, field["name"])
			out.append(int(field["code"]))
	_put_u16(out, collections.size())
	for i in range(collections.size()):
		_put_string16(out, collections[i]["name"])
		_put_u16(out, i)
	return out


static func _put_u16(out: PackedByteArray, value: int) -> void:
	out.append(value & 0xFF)
	out.append((value >> 8) & 0xFF)


static func _put_u32(out: PackedByteArray, value: int) -> void:
	out.append(value & 0xFF)
	out.append((value >> 8) & 0xFF)
	out.append((value >> 16) & 0xFF)
	out.append((value >> 24) & 0xFF)


static func _put_string16(out: PackedByteArray, text: String) -> void:
	var bytes := text.to_utf8_buffer()
	_put_u16(out, bytes.size())
	out.append_array(bytes)
