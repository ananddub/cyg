class_name CYGSyncArray
extends RefCounted
## Networked ordered list that ships element-level diffs instead of the whole list.
##
## Authority-side methods mutate the list and queue one op per change; proxy-side
## `deserialize()` replays those ops in the sender's order. Element indices in a
## queued op are only meaningful relative to the ops before it, so the queue is
## never reordered, and adjacent SET records are the only ones the writer groups --
## grouping cannot shift an index, so the batch means the same thing on every peer.
##
## Delta blob, byte-exact (`[varint nextRevision][varint mutationCount]` comes from
## `CYGDeltaPacker.write_batch()`, then one entry per op, then the `OP_END` byte):
##
##     [u8 OP_SET        = 1] [varint index] [value]
##     [u8 OP_SET_MANY   = 2] [varint baseIndex] [mask] [value] * popcount(mask)
##     [u8 OP_INSERT     = 3] [varint index] [value]
##     [u8 OP_REMOVE     = 4] [varint index]
##     [u8 OP_CLEAR      = 5]
##     [u8 OP_SNAPSHOT   = 6] [varint length] [value] * length
##     [u8 OP_END        = 0]
##
## `[value]` is `CYGDeltaPacker`'s tagged codec (`[u8 tag][payload]`, tag =
## `typeof(value)`), so the list may mix `int`, `String`, `Vector3` and nested
## `Array`/`Dictionary` elements. `[mask]` is `[varint slotCount][bytes LSB-first]`
## from `CYGDeltaPacker`, and the number of values that follows is the mask's
## population count, read from the mask itself rather than from a local notion of
## how many changes the receiver cares about.
##
## `mutationCount` is the sender's mutation count for the batch, which lets a
## receiver spot a dropped delta: `localRevision + mutationCount != nextRevision`
## means ops are missing. The receiver then decodes the whole blob (so the shared
## `CYGBitReader` stays aligned for the next component) but applies nothing, and
## raises `needs_snapshot()` for the component to answer with `write_full_state()`.
##
## Usage, inside a `CYGBehaviour` component:
##
##     var _inventory := CYGSyncArray.new()
##
##     func _ready() -> void:
##         _inventory.element_added.connect(_on_item_added)
##         _inventory.element_removed.connect(_on_item_removed)
##         register_as_observable()
##
##     func add_item(item_id: String) -> void:
##         if has_authority:
##             _inventory.append(item_id)
##
##     func has_delta() -> bool:
##         return _inventory.has_delta() or super.has_delta()
##
##     func serialize(writer: CYGBitPacker) -> void:
##         super.serialize(writer)
##         _inventory.serialize(writer)
##
##     func deserialize(reader: CYGBitReader) -> void:
##         super.deserialize(reader)
##         if not _inventory.deserialize(reader) and _inventory.needs_snapshot():
##             server_rpc("request_inventory_snapshot")
##
## Godot's typed `Array[X]` cannot hold arbitrary variants, so the list is untyped
## internally and every element is validated against `CYGDeltaPacker.supports()`
## before it is queued.
##
## Design derives from Netfox's MIT-licensed sync collections and Mirror's SyncList
## dirty ops (attribution in LICENSE-NETFOX.md). Clean-room GDScript; no upstream
## source text is copied.

signal element_added(index: int, value: Variant)
signal element_updated(index: int, old_value: Variant, new_value: Variant)
signal element_removed(index: int, value: Variant)
signal cleared(previous: Array)

const OP_END := 0
const OP_SET := 1
const OP_SET_MANY := 2
const OP_INSERT := 3
const OP_REMOVE := 4
const OP_CLEAR := 5
const OP_SNAPSHOT := 6

## Queue records are `[op, index, value]`, `[op, index]` or `[op]` in an untyped
## outer Array, which keeps the diff log compact and avoids an inner class per op.
const _IDX_OP := 0
const _IDX_INDEX := 1
const _IDX_VALUE := 2

var _elements: Array = []
var _queue: Array = []
var _revision := 0
var _needs_snapshot := false
# True while a blob is being applied; false while a stale blob is only being
# decoded for byte alignment.
var _applying := true

# Reusable mask/header scratch, so a flush that coalesces sets allocates nothing.
var _packer: CYGDeltaPacker = CYGDeltaPacker.new()


func _init(initial: Array = []) -> void:
	for value in initial:
		if not CYGDeltaPacker.supports(value):
			push_error("CYGSyncArray: cannot seed an element of type %d." % typeof(value))
			continue
		_elements.append(value)


## Element count.
func size() -> int:
	return _elements.size()


func is_empty() -> bool:
	return _elements.is_empty()


## Monotonic change counter; a receiver compares it against a delta's declared
## revision to detect a dropped batch.
func revision() -> int:
	return _revision


## Number of unsent mutations, for a component's send telemetry.
func pending_mutation_count() -> int:
	return _queue.size()


func at(index: int) -> Variant:
	if index < 0 or index >= _elements.size():
		push_error("CYGSyncArray.at: index %d is outside a list of %d elements." % [index, _elements.size()])
		return null
	return _elements[index]


## Alias of `at()`, spelled like the SDK's other accessors.
func get_element(index: int) -> Variant:
	return at(index)


func find(value: Variant) -> int:
	return _elements.find(value)


func has_element(value: Variant) -> bool:
	return _elements.has(value)


## Deep copy of the list, safe to keep.
func to_array() -> Array:
	return _elements.duplicate(true)


## Authority-side overwrite of one element. Equal values are dropped.
func set_element(index: int, value: Variant) -> bool:
	if index < 0 or index >= _elements.size():
		push_error("CYGSyncArray.set_element: index %d is outside a list of %d elements." % [index, _elements.size()])
		return false
	if not _check_value(value):
		return false
	var previous: Variant = _elements[index]
	if CYGDeltaPacker.values_equal(previous, value):
		return true
	_record(OP_SET, index, value)
	_set_local(index, value, previous)
	return true


## Authority-side insert at `index`, which may be `size()` to append.
func insert(index: int, value: Variant) -> bool:
	if index < 0 or index > _elements.size():
		push_error("CYGSyncArray.insert: index %d is outside 0..%d." % [index, _elements.size()])
		return false
	if not _check_value(value):
		return false
	_record(OP_INSERT, index, value)
	_elements.insert(index, value)
	element_added.emit(index, value)
	return true


func append(value: Variant) -> bool:
	return insert(_elements.size(), value)


func push_back(value: Variant) -> bool:
	return append(value)


func push_front(value: Variant) -> bool:
	return insert(0, value)


## Removes and returns the element at `index`.
func remove_at(index: int) -> Variant:
	if index < 0 or index >= _elements.size():
		push_error("CYGSyncArray.remove_at: index %d is outside a list of %d elements." % [index, _elements.size()])
		return null
	var removed: Variant = _elements[index]
	_record(OP_REMOVE, index, null)
	_elements.remove_at(index)
	element_removed.emit(index, removed)
	return removed


func pop_back() -> Variant:
	if _elements.is_empty():
		push_error("CYGSyncArray.pop_back: the list is empty.")
		return null
	return remove_at(_elements.size() - 1)


func pop_front() -> Variant:
	if _elements.is_empty():
		push_error("CYGSyncArray.pop_front: the list is empty.")
		return null
	return remove_at(0)


## Removes the first occurrence of `value`.
func erase(value: Variant) -> bool:
	var index := _elements.find(value)
	if index < 0:
		return false
	remove_at(index)
	return true


## Authority-side bulk replace of the whole list as one mutation.
func clear() -> void:
	_record(OP_CLEAR, 0, null)
	_clear_local()


## The full list a late joiner must start from.
func read_full_state() -> Array:
	return to_array()


## Replaces the list from decoded full state (a spawn payload). Emits `cleared`
## then one `element_added` per surviving element.
func apply_full_state(values: Array) -> void:
	_queue.clear()
	_revision += 1
	_needs_snapshot = false
	var previous := _elements
	_elements = values.duplicate(true)
	cleared.emit(previous)
	for i in range(_elements.size()):
		element_added.emit(i, _elements[i])


## True when at least one mutation is waiting for the next tick.
func has_delta() -> bool:
	return not _queue.is_empty()


## Sets `needs_snapshot()`; call it after answering a request so the component does
## not keep re-asking.
func needs_snapshot() -> bool:
	return _needs_snapshot


func clear_snapshot_request() -> void:
	_needs_snapshot = false


## Writes `[header][entries][OP_END]`, draining the queue.
func serialize(writer: CYGBitPacker) -> void:
	var mutations := _queue.size()
	CYGDeltaPacker.write_batch(writer, _revision, mutations)
	var i := 0
	while i < mutations:
		var entry: Array = _queue[i]
		var run := 1
		# Only adjacent sets merge: a merge between a set and anything else could
		# change what an index means mid-batch.
		if int(entry[_IDX_OP]) == OP_SET:
			while i + run < mutations and int((_queue[i + run] as Array)[_IDX_OP]) == OP_SET:
				run += 1
			_write_set_run(writer, i, run)
		else:
			_write_entry(writer, entry)
		i += run
	writer.write_byte(OP_END)
	_queue.clear()


## Writes the whole list as one `OP_SNAPSHOT` batch, discarding queued diffs: the
## snapshot subsumes them, and every receiver jumps to the declared revision.
func write_full_state(writer: CYGBitPacker) -> void:
	var length := _elements.size()
	_queue.clear()
	_revision += 1
	CYGDeltaPacker.write_batch(writer, _revision, 1)
	if length > CYGDeltaPacker.MAX_COUNT:
		push_error("CYGSyncArray: %d elements exceed the %d wire limit; send it in slices." % [length, CYGDeltaPacker.MAX_COUNT])
		writer.write_byte(OP_END)
		return
	writer.write_byte(OP_SNAPSHOT)
	writer.write_uint32(length)
	for i in range(length):
		CYGDeltaPacker.write_value(writer, _elements[i])
	writer.write_byte(OP_END)
	_needs_snapshot = false


## Replays one blob written by `serialize()` or `write_full_state()`. Returns false
## when the batch was stale or undecodable and `needs_snapshot()` is now set; the
## blob is always consumed in full unless an unknown op makes it unmeasurable.
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
		push_warning("CYGSyncArray: revision gap (local %d + %d != %d); skipping the batch and requesting a full state."
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
		push_error("CYGSyncArray: blob could not be decoded at op %d; the rest of this frame is unreadable, requesting a full state." % op)
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
		OP_SET, OP_INSERT:
			var single_index := reader.read_uint32()
			var single_value: Variant = CYGDeltaPacker.read_value(reader)
			if reader.last_read_failed:
				return false
			if not _applying:
				return true
			if op == OP_SET:
				_apply_set(single_index, single_value)
			else:
				_apply_insert(single_index, single_value)
			return true
		OP_SET_MANY:
			var base := reader.read_uint32()
			if not _packer.read_mask(reader):
				return false
			# Every marked slot carries a value whether or not the index is usable
			# here: skipping the reads is how one peer's cursor stops matching
			# another's.
			var many_slot: int = _packer.first_marked()
			while many_slot >= 0:
				var many_value: Variant = CYGDeltaPacker.read_value(reader)
				if reader.last_read_failed:
					return false
				if _applying:
					_apply_set(base + many_slot, many_value)
				many_slot = _packer.next_marked(many_slot)
			return true
		OP_REMOVE:
			var remove_index := reader.read_uint32()
			if reader.last_read_failed:
				return false
			if _applying:
				_apply_remove(remove_index)
			return true
		OP_CLEAR:
			if _applying:
				_clear_local()
			return true
		OP_SNAPSHOT:
			var length := reader.read_uint32()
			if length < 0 or length > CYGDeltaPacker.MAX_COUNT:
				push_error("CYGSyncArray: snapshot declares %d elements (limit %d)." % [length, CYGDeltaPacker.MAX_COUNT])
				return false
			var values: Array = []
			for i in range(length):
				values.append(CYGDeltaPacker.read_value(reader))
				if reader.last_read_failed:
					return false
			if _applying:
				apply_full_state(values)
			return true
		_:
			push_error("CYGSyncArray: unknown op %d; its payload length is unknowable, so this blob cannot be skipped." % op)
			return false


func _apply_set(index: int, value: Variant) -> void:
	if index < 0 or index >= _elements.size():
		push_warning("CYGSyncArray: remote set at index %d is outside a list of %d elements." % [index, _elements.size()])
		_needs_snapshot = true
		return
	_set_local(index, value, _elements[index])


func _apply_insert(index: int, value: Variant) -> void:
	if index < 0 or index > _elements.size():
		push_warning("CYGSyncArray: remote insert at index %d is outside 0..%d." % [index, _elements.size()])
		_needs_snapshot = true
		return
	_elements.insert(index, value)
	element_added.emit(index, value)


func _apply_remove(index: int) -> void:
	if index < 0 or index >= _elements.size():
		push_warning("CYGSyncArray: remote remove at index %d is outside a list of %d elements." % [index, _elements.size()])
		_needs_snapshot = true
		return
	var removed: Variant = _elements[index]
	_elements.remove_at(index)
	element_removed.emit(index, removed)


func _set_local(index: int, value: Variant, previous: Variant) -> void:
	_elements[index] = value
	element_updated.emit(index, previous, value)


func _clear_local() -> void:
	var previous := _elements
	_elements = []
	cleared.emit(previous)


## Queues one op. The revision moves with the mutation, so a receiver can count
## the changes it missed.
func _record(op: int, index: int, value: Variant) -> void:
	if op == OP_CLEAR or op == OP_REMOVE:
		_queue.append([op, index])
	else:
		_queue.append([op, index, value])
	_revision += 1


func _write_entry(writer: CYGBitPacker, entry: Array) -> void:
	var op := int(entry[_IDX_OP])
	writer.write_byte(op)
	match op:
		OP_SET, OP_INSERT:
			writer.write_uint32(int(entry[_IDX_INDEX]))
			CYGDeltaPacker.write_value(writer, entry[_IDX_VALUE])
		OP_REMOVE:
			writer.write_uint32(int(entry[_IDX_INDEX]))
		_:
			pass


## Emits `run` adjacent SET records as one masked batch when the mask is cheaper
## than an op byte plus an index per change, and as plain ops otherwise.
func _write_set_run(writer: CYGBitPacker, start: int, run: int) -> void:
	var lo := int((_queue[start] as Array)[_IDX_INDEX])
	var hi := lo
	for k in range(start + 1, start + run):
		var index := int((_queue[k] as Array)[_IDX_INDEX])
		lo = mini(lo, index)
		hi = maxi(hi, index)
	var span := hi - lo + 1
	var mask_bytes := (span + 7) >> 3
	# Individual ops cost about two bytes per change; the mask costs one byte per
	# eight slots, so only a dense cluster of at least two changes is worth it.
	if run > 1 and span <= CYGDeltaPacker.MAX_SLOTS and mask_bytes * 2 <= run:
		_packer.configure(span)
		for k in range(start, start + run):
			_packer.mark(int((_queue[k] as Array)[_IDX_INDEX]) - lo)
		# A mask cannot express two writes to the same index, so a run that repeats
		# an index falls back to ordered per-index ops.
		if _packer.marked_count() == run:
			writer.write_byte(OP_SET_MANY)
			writer.write_uint32(lo)
			_packer.write_mask(writer)
			var marked: int = _packer.first_marked()
			while marked >= 0:
				CYGDeltaPacker.write_value(writer, _last_value_for(start, run, lo + marked))
				marked = _packer.next_marked(marked)
			return
	for k in range(start, start + run):
		_write_entry(writer, _queue[k])


## Latest queued value for `index` inside the run, which is the one that survives.
func _last_value_for(start: int, run: int, index: int) -> Variant:
	var found: Variant = null
	for k in range(start, start + run):
		var entry: Array = _queue[k]
		if int(entry[_IDX_INDEX]) == index:
			found = entry[_IDX_VALUE]
	return found


func _check_value(value: Variant) -> bool:
	if not CYGDeltaPacker.supports(value):
		push_error("CYGSyncArray: cannot sync an element of type %d." % typeof(value))
		return false
	return true
