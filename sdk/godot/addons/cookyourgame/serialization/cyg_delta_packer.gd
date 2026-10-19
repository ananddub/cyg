class_name CYGDeltaPacker
extends RefCounted
## Dirty-slot bitmask and tagged-value diff primitives shared by the sync
## collections (`CYGSyncProperty`, `CYGSyncArray`, `CYGSyncDict`).
##
## Three jobs, all receiver-independent so every peer consumes identical bytes:
##
## 1. Bitmask over N slots. `configure(slots)` sizes it, `mark(slot)` flags one,
##    `write_mask()` emits `[varint slotCount][ceil(slotCount/8) bytes, LSB-first
##    inside each byte, ascending byte order]`. `read_mask()` refills a
##    receiver-side packer, whose `first_marked()`/`next_marked()` walk says
##    exactly how many tagged payloads follow. This is the compressed form of
##    "these element indices changed": 1 bit per slot instead of an op byte plus
##    an index per change.
##
## 2. Delta frame header. `write_batch`/`read_batch_header` emit and parse
##    `[varint nextRevision][varint mutationCount]`, the pair every collection
##    needs to detect a dropped delta.
##
## 3. Tagged value codec. `[u8 tag][payload]` where the tag is `typeof(value)`,
##    scalar payloads are delegated to `CYGPropertyMap.write_value()`/
##    `read_value()` so the SDK keeps exactly one scalar encoding, and
##    `TYPE_ARRAY`/`TYPE_DICTIONARY` recurse behind a `varint` element count.
##    `TYPE_NIL` is a bare tag. An unknown tag is a hard `push_error` with no
##    fallback value: a payload the reader cannot measure is never guessed at,
##    because guessing is how one desync becomes ten.
##
## A component never talks to this class: it owns one of the three sync
## collections, which keeps a packer instance as reusable scratch so a tick that
## changes nothing allocates nothing.
##
## Design derives from Netfox's MIT-licensed sync collections and Mirror's
## SyncList dirty masking (attribution in LICENSE-NETFOX.md). Independent
## GDScript; no upstream source text is copied.

## Mask geometry limit: 4096 slots is 512 wire bytes. Above that the caller must
## fall back to per-index ops, since a wide sparse mask costs more than the ops.
const MAX_SLOTS := 4096
## Wire-count ceiling for a container read. A larger declared count cannot be
## skipped, so the read fails loudly instead of allocating on corrupt input.
const MAX_COUNT := 4096

# Tags are the engine's own TYPE_* values, so `typeof()` *is* the tag and the
# scalar codec needs no translation table.
const TAG_NIL := TYPE_NIL
const TAG_BOOL := TYPE_BOOL
const TAG_INT := TYPE_INT
const TAG_FLOAT := TYPE_FLOAT
const TAG_STRING := TYPE_STRING
const TAG_VECTOR2 := TYPE_VECTOR2
const TAG_VECTOR3 := TYPE_VECTOR3
const TAG_QUATERNION := TYPE_QUATERNION
const TAG_COLOR := TYPE_COLOR
const TAG_BYTES := TYPE_PACKED_BYTE_ARRAY
const TAG_ARRAY := TYPE_ARRAY
const TAG_DICT := TYPE_DICTIONARY

# The mask buffer grows to the widest batch ever seen and is then reused, so the
# per-tick flush writes bits without resizing.
var _bits := PackedByteArray()
var _slots := 0

## `read_batch_header()` results; scratch, valid until the next read.
var revision: int = 0
var mutation_count: int = 0

static var _nibble_bits := PackedInt32Array([0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4])


## Sizes the mask to `slot_count` slots and clears every bit. Never shrinks the
## backing buffer, only the logical width.
func configure(slot_count: int) -> void:
	_slots = maxi(slot_count, 0)
	var used := _bytes_for(_slots)
	if _bits.size() < used:
		_bits.resize(used)
	for i in range(used):
		_bits[i] = 0


func slot_count() -> int:
	return _slots


## Flags `slot` as changed. Out-of-range slots are an error rather than a silent
## widening of the mask, because the receiver's value count comes from the mask.
func mark(slot: int) -> void:
	if slot < 0 or slot >= _slots:
		push_error("CYGDeltaPacker.mark: slot %d is outside %d configured slots." % [slot, _slots])
		return
	var byte_index := slot >> 3
	var bits: int = _bits[byte_index]
	bits |= 1 << (slot & 7)
	_bits[byte_index] = bits


## Set bits inside the configured width. This is the population count a masked
## batch's payload is sized by.
func marked_count() -> int:
	var total := 0
	var full_bytes := _slots >> 3
	for i in range(full_bytes):
		total += _count_bits(int(_bits[i]))
	# The final byte is partial whenever the width is not a multiple of eight; its
	# high bits belong to no slot and must not be counted.
	if _slots & 7 != 0:
		var mask := (1 << (_slots & 7)) - 1
		total += _count_bits(int(_bits[full_bytes]) & mask)
	return total


## Lowest marked slot, or -1 when the mask is empty.
func first_marked() -> int:
	return next_marked(-1)


## Next marked slot strictly after `slot`, or -1 when the mask is exhausted.
func next_marked(slot: int) -> int:
	var from := maxi(slot + 1, 0)
	for i in range(from, _slots):
		if int(_bits[i >> 3]) & (1 << (i & 7)) != 0:
			return i
	return -1


## `[varint slotCount][mask bytes]`, ascending byte order.
func write_mask(writer: CYGBitPacker) -> void:
	writer.write_uint32(_slots)
	var used := _bytes_for(_slots)
	for i in range(used):
		writer.write_byte(int(_bits[i]))


## Reads a mask written by `write_mask()` into this packer's scratch, so the
## caller can walk the set bits without allocating.
func read_mask(reader: CYGBitReader) -> bool:
	var slots := reader.read_uint32()
	if slots < 0 or slots > MAX_SLOTS:
		push_error("CYGDeltaPacker: mask declares %d slots (limit %d); the stream is out of sync."
			% [slots, MAX_SLOTS])
		return false
	_slots = slots
	var used := _bytes_for(slots)
	if _bits.size() < used:
		_bits.resize(used)
	for i in range(used):
		_bits[i] = reader.read_byte()
	return not reader.last_read_failed


## Collection delta header: `[varint nextRevision][varint mutationCount]`.
static func write_batch(writer: CYGBitPacker, next_revision: int, mutation_count: int) -> void:
	writer.write_uint64(next_revision)
	writer.write_uint32(maxi(mutation_count, 0))


## Parses the delta header into `revision`/`mutation_count`.
func read_batch_header(reader: CYGBitReader) -> bool:
	revision = reader.read_uint64()
	mutation_count = reader.read_uint32()
	if mutation_count < 0 or mutation_count > MAX_COUNT:
		push_error("CYGDeltaPacker: delta header declares %d mutations (limit %d)."
			% [mutation_count, MAX_COUNT])
		mutation_count = 0
		return false
	return not reader.last_read_failed


## Writes `[u8 tag][payload]` for one value.
static func write_value(writer: CYGBitPacker, value: Variant) -> void:
	var tag := typeof(value)
	writer.write_byte(tag)
	match tag:
		TAG_NIL:
			pass
		TAG_ARRAY:
			var list: Array = value
			writer.write_uint32(list.size())
			for i in range(list.size()):
				write_value(writer, list[i])
		TAG_DICT:
			var table: Dictionary = value
			writer.write_uint32(table.size())
			# WHY sorted: Godot keeps a Dictionary's insertion order, and that order
			# diverges between peers as soon as one removes and re-adds a key. The wire
			# order may not inherit it.
			var names: Array = table.keys()
			sort_keys(names)
			for key in names:
				write_value(writer, key)
				write_value(writer, table[key])
		_:
			if not _is_scalar(tag):
				push_error("CYGDeltaPacker: cannot encode a value of type %d; sync values must be scalar, Array or Dictionary." % tag)
				return
			CYGPropertyMap.write_value(writer, tag, value)


## Reads one `[u8 tag][payload]` written by `write_value()`. Returns null on an
## undecodable stream, which the caller must treat as "resync me", never as data.
static func read_value(reader: CYGBitReader) -> Variant:
	var tag := reader.read_byte()
	if reader.last_read_failed:
		return null
	match tag:
		TAG_NIL:
			return null
		TAG_ARRAY:
			var list_count := reader.read_uint32()
			if list_count < 0 or list_count > MAX_COUNT:
				push_error("CYGDeltaPacker: nested Array declares %d elements (limit %d); cannot skip it."
					% [list_count, MAX_COUNT])
				return null
			var list: Array = []
			for i in range(list_count):
				list.append(read_value(reader))
				if reader.last_read_failed:
					return null
			return list
		TAG_DICT:
			var table_count := reader.read_uint32()
			if table_count < 0 or table_count > MAX_COUNT:
				push_error("CYGDeltaPacker: nested Dictionary declares %d entries (limit %d); cannot skip it."
					% [table_count, MAX_COUNT])
				return null
			var table := {}
			for i in range(table_count):
				var key := read_value(reader)
				var value := read_value(reader)
				if reader.last_read_failed:
					return null
				table[key] = value
			return table
		_:
			if not _is_scalar(tag):
				push_error("CYGDeltaPacker: unknown value tag %d; the payload cannot be measured, so the rest of the stream is unreadable." % tag)
				return null
			return CYGPropertyMap.read_value(reader, tag)


## True when `value` can reach the wire. Collections reject other types at
## mutation time, so an Object or Signal never queues an unsendable op.
static func supports(value: Variant) -> bool:
	var tag := typeof(value)
	return tag == TAG_NIL or tag == TAG_ARRAY or tag == TAG_DICT or _is_scalar(tag)


## Type-safe equality for "did this change?" checks. Godot raises a runtime error
## on `Vector3 == "text"`, and a sync collection compares a stored value against a
## fresh one whose type the writer may legitimately change, so mismatched types are
## simply unequal rather than compared.
static func values_equal(a: Variant, b: Variant) -> bool:
	if typeof(a) != typeof(b):
		return false
	return a == b


## Deterministic ordering for keys that reach the wire. The tag is compared first
## because a mixed dictionary holds int *and* String keys, which cannot be
## ordered against each other; `str()` is the tiebreak for the rest.
static func sort_keys(keys: Array) -> void:
	keys.sort_custom(_key_less)


static func _key_less(a: Variant, b: Variant) -> bool:
	var tag_a := typeof(a)
	var tag_b := typeof(b)
	if tag_a != tag_b:
		return tag_a < tag_b
	match tag_a:
		TYPE_STRING:
			return String(a) < String(b)
		TYPE_INT, TYPE_BOOL:
			return int(a) < int(b)
		TYPE_FLOAT:
			return float(a) < float(b)
		_:
			return str(a) < str(b)


static func _is_scalar(tag: int) -> bool:
	return tag in [
		TYPE_BOOL, TYPE_INT, TYPE_FLOAT, TYPE_STRING, TYPE_VECTOR2,
		TYPE_VECTOR3, TYPE_QUATERNION, TYPE_COLOR, TYPE_PACKED_BYTE_ARRAY,
	]


static func _bytes_for(slots: int) -> int:
	return (slots + 7) >> 3


static func _count_bits(byte: int) -> int:
	return _nibble_bits[byte & 0xF] + _nibble_bits[byte >> 4]
