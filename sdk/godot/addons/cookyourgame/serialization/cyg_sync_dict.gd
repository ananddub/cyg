class_name CYGSyncDict
extends RefCounted
## Networked `Dictionary` keyed by `String`, shipping key-level diffs.
##
## Mirrors the SDK's property-naming idiom (`CYGPropertyMap` addresses entries by
## name), but holds arbitrary variants per key and tags every one of them, so a
## room-level bag can carry an `int`, a `Vector3` and a nested `Array` side by
## side. Authority-side methods queue one op per change; `deserialize()` replays
## them in order.
##
## Delta blob, byte-exact (`[varint nextRevision][varint mutationCount]` comes from
## `CYGDeltaPacker.write_batch()`, then one entry per op, then the `OP_END` byte):
##
##     [u8 OP_SET      = 1] [string key] [value]
##     [u8 OP_REMOVE   = 2] [string key]
##     [u8 OP_CLEAR    = 3]
##     [u8 OP_SNAPSHOT = 4] [varint count] ( [string key] [value] ) * count
##     [u8 OP_END      = 0]
##
## `[string key]` is `CYGBitPacker.write_string()`: `[varint utf8Length][bytes]`.
## `[value]` is `CYGDeltaPacker`'s tagged codec (`[u8 tag][payload]`), so a key's
## type may change between deltas and still round-trips.
##
## Deltas are op-based, so nothing about them depends on `Dictionary` iteration
## order. The one place order does reach the wire is the snapshot, whose keys are
## sorted explicitly with `CYGDeltaPacker.sort_keys()`: two peers that removed and
## re-added the same keys in different bursts would otherwise send different byte
## streams for the same content.
##
## As with `CYGSyncArray`, `mutationCount` lets a receiver detect a dropped batch
## (`localRevision + mutationCount != nextRevision`): the blob is decoded in full so
## the shared `CYGBitReader` stays aligned, nothing is applied, and `needs_snapshot()`
## is raised until the component answers with `write_full_state()`.
##
## Usage, inside a `CYGBehaviour` component:
##
##     var _loadout := CYGSyncDict.new()
##
##     func _ready() -> void:
##         _loadout.entry_updated.connect(_on_slot_changed)
##         register_as_observable()
##
##     func has_delta() -> bool:
##         return _loadout.has_delta() or super.has_delta()
##
##     func serialize(writer: CYGBitPacker) -> void:
##         super.serialize(writer)
##         _loadout.serialize(writer)
##
##     func deserialize(reader: CYGBitReader) -> void:
##         super.deserialize(reader)
##         if not _loadout.deserialize(reader) and _loadout.needs_snapshot():
##             server_rpc("request_loadout")
##
## Design derives from Netfox's MIT-licensed sync collections and Mirror's
## SyncDictionary dirty ops (attribution in LICENSE-NETFOX.md). Clean-room
## GDScript; no upstream source text is copied.

signal entry_added(key: String, value: Variant)
signal entry_updated(key: String, old_value: Variant, new_value: Variant)
signal entry_removed(key: String, value: Variant)
signal cleared(previous: Dictionary)

const OP_END := 0
const OP_SET := 1
const OP_REMOVE := 2
const OP_CLEAR := 3
const OP_SNAPSHOT := 4

# Queue records are `[op, key]` or `[op, key, value]`.
const _IDX_OP := 0
const _IDX_KEY := 1
const _IDX_VALUE := 2

var _entries := {}
var _queue: Array = []
var _revision := 0
var _needs_snapshot := false
var _applying := true

var _packer: CYGDeltaPacker = CYGDeltaPacker.new()


func _init(initial: Dictionary = {}) -> void:
	for key in initial:
		if typeof(key) != TYPE_STRING:
			push_error("CYGSyncDict: seeding key '%s' is not a String; sync dictionaries are String-keyed." % str(key))
			continue
		if not CYGDeltaPacker.supports(initial[key]):
			push_error("CYGSyncDict: cannot seed key '%s' with type %d." % [key, typeof(initial[key])])
			continue
		_entries[key] = initial[key]


func size() -> int:
	return _entries.size()


func is_empty() -> bool:
	return _entries.is_empty()


## Monotonic change counter; compared against a delta's declared revision to spot
## a batch this peer never saw.
func revision() -> int:
	return _revision


## Number of unsent mutations, for a component's send telemetry.
func pending_mutation_count() -> int:
	return _queue.size()


func has(key: String) -> bool:
	return _entries.has(key)


## The value stored for `key`, or `fallback` when the key is absent.
func get_value(key: String, fallback: Variant = null) -> Variant:
	return _entries[key] if _entries.has(key) else fallback


## Keys in insertion order. Both peers apply the same op stream, so the order they
## build is the same; anything that reaches the wire sorts explicitly instead.
func keys() -> Array:
	return _entries.keys()


## Ascending keys, the order a snapshot travels in.
func sorted_keys() -> Array:
	var names: Array = _entries.keys()
	CYGDeltaPacker.sort_keys(names)
	return names


## Deep copy, safe to keep.
func to_dictionary() -> Dictionary:
	return _entries.duplicate(true)


## Authority-side write of one key. Adding a new key and overwriting an existing
## one are distinct ops on the signals, never on the wire.
func set_value(key: String, value: Variant) -> bool:
	if key.is_empty():
		push_error("CYGSyncDict.set_value: the empty string is not a valid key.")
		return false
	if not CYGDeltaPacker.supports(value):
		push_error("CYGSyncDict.set_value: cannot sync type %d under key '%s'." % [typeof(value), key])
		return false
	var is_new: bool = not _entries.has(key)
	var previous: Variant = _entries[key] if not is_new else null
	if not is_new and CYGDeltaPacker.values_equal(previous, value):
		return true
	_queue.append([OP_SET, key, value])
	_revision += 1
	_entries[key] = value
	if is_new:
		entry_added.emit(key, value)
	else:
		entry_updated.emit(key, previous, value)
	return true


## Removes `key` and returns its former value, or null when it was absent.
func remove(key: String) -> Variant:
	if not _entries.has(key):
		return null
	var previous: Variant = _entries[key]
	_queue.append([OP_REMOVE, key])
	_revision += 1
	_entries.erase(key)
	entry_removed.emit(key, previous)
	return previous


## Authority-side bulk wipe, recorded as one mutation.
func clear() -> void:
	_queue.append([OP_CLEAR])
	_revision += 1
	_clear_local()


## The full map a late joiner must start from.
func read_full_state() -> Dictionary:
	return to_dictionary()


## Replaces the map from decoded full state (a spawn payload). Emits `cleared`,
## then one `entry_added` per surviving key in ascending order.
func apply_full_state(values: Dictionary) -> void:
	_queue.clear()
	_revision += 1
	_needs_snapshot = false
	var previous := _entries
	_entries = values.duplicate(true)
	cleared.emit(previous)
	for key in sorted_keys():
		entry_added.emit(key, _entries[key])


func has_delta() -> bool:
	return not _queue.is_empty()


## True when the last blob was stale or undecodable and a full state is owed.
func needs_snapshot() -> bool:
	return _needs_snapshot


func clear_snapshot_request() -> void:
	_needs_snapshot = false


## Writes `[header][entries][OP_END]`, draining the queue.
func serialize(writer: CYGBitPacker) -> void:
	var mutations := _queue.size()
	CYGDeltaPacker.write_batch(writer, _revision, mutations)
	for i in range(mutations):
		_write_entry(writer, _queue[i])
	writer.write_byte(OP_END)
	_queue.clear()


## Writes the whole map as one `OP_SNAPSHOT` batch, discarding queued diffs.
func write_full_state(writer: CYGBitPacker) -> void:
	var count := _entries.size()
	_queue.clear()
	_revision += 1
	CYGDeltaPacker.write_batch(writer, _revision, 1)
	if count > CYGDeltaPacker.MAX_COUNT:
		push_error("CYGSyncDict: %d keys exceed the %d wire limit; partition the dictionary." % [count, CYGDeltaPacker.MAX_COUNT])
		writer.write_byte(OP_END)
		return
	writer.write_byte(OP_SNAPSHOT)
	writer.write_uint32(count)
	var names: Array = _entries.keys()
	CYGDeltaPacker.sort_keys(names)
	for key in names:
		writer.write_string(key)
		CYGDeltaPacker.write_value(writer, _entries[key])
	writer.write_byte(OP_END)
	_needs_snapshot = false


## Replays one blob written by `serialize()` or `write_full_state()`. Returns false
## when the batch was stale or undecodable; a stale blob is still consumed in full.
func deserialize(reader: CYGBitReader) -> bool:
	if not _packer.read_batch_header(reader):
		_needs_snapshot = true
		return false
	var next_revision: int = _packer.revision
	var mutations: int = _packer.mutation_count
	var first_op := reader.read_byte()
	if reader.last_read_failed:
		_needs_snapshot = true
		return false
	var stale := mutations > 0 and first_op != OP_SNAPSHOT and _revision + mutations != next_revision
	if stale:
		push_warning("CYGSyncDict: revision gap (local %d + %d != %d); skipping the batch and requesting a full state."
			% [_revision, mutations, next_revision])
		_needs_snapshot = true
	_applying = not stale
	var op := first_op
	var ok := true
	while op != OP_END:
		if not _apply_entry(op, reader):
			ok = false
			break
		op = reader.read_byte()
		if reader.last_read_failed:
			ok = false
			break
	_applying = true
	if not ok:
		push_error("CYGSyncDict: blob could not be decoded at op %d; the rest of this frame is unreadable, requesting a full state." % op)
		_needs_snapshot = true
		return false
	if stale:
		return false
	_revision = next_revision
	if first_op == OP_SNAPSHOT:
		_needs_snapshot = false
	return true


func _apply_entry(op: int, reader: CYGBitReader) -> bool:
	match op:
		OP_SET:
			var entry_key := reader.read_string()
			var entry_value: Variant = CYGDeltaPacker.read_value(reader)
			if reader.last_read_failed:
				return false
			if _applying:
				_apply_set(entry_key, entry_value)
			return true
		OP_REMOVE:
			var remove_key := reader.read_string()
			if reader.last_read_failed:
				return false
			if _applying and _entries.has(remove_key):
				var previous: Variant = _entries[remove_key]
				_entries.erase(remove_key)
				entry_removed.emit(remove_key, previous)
			return true
		OP_CLEAR:
			if _applying:
				_clear_local()
			return true
		OP_SNAPSHOT:
			var count := reader.read_uint32()
			if count < 0 or count > CYGDeltaPacker.MAX_COUNT:
				push_error("CYGSyncDict: snapshot declares %d keys (limit %d)." % [count, CYGDeltaPacker.MAX_COUNT])
				return false
			var values := {}
			for i in range(count):
				var snap_key := reader.read_string()
				var snap_value: Variant = CYGDeltaPacker.read_value(reader)
				if reader.last_read_failed:
					return false
				values[snap_key] = snap_value
			if _applying:
				apply_full_state(values)
			return true
		_:
			push_error("CYGSyncDict: unknown op %d; its payload length is unknowable, so this blob cannot be skipped." % op)
			return false


func _apply_set(key: String, value: Variant) -> void:
	var is_new: bool = not _entries.has(key)
	var previous: Variant = null if is_new else _entries[key]
	_entries[key] = value
	if is_new:
		entry_added.emit(key, value)
	else:
		entry_updated.emit(key, previous, value)


func _clear_local() -> void:
	var previous := _entries
	_entries = {}
	cleared.emit(previous)


func _write_entry(writer: CYGBitPacker, entry: Array) -> void:
	var op := int(entry[_IDX_OP])
	writer.write_byte(op)
	match op:
		OP_SET:
			writer.write_string(entry[_IDX_KEY])
			CYGDeltaPacker.write_value(writer, entry[_IDX_VALUE])
		OP_REMOVE:
			writer.write_string(entry[_IDX_KEY])
		_:
			pass
