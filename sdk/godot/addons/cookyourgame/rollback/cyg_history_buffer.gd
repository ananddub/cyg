class_name CYGHistoryBuffer
extends RefCounted
## Fixed-capacity ring of `{tick -> opaque state}` backing resimulation and
## lag-compensation queries.
##
## A tick maps directly onto `tick & mask`, so `push()` and `get_state()` are O(1)
## and allocate nothing: state lives in typed parallel arrays, exactly like the
## snapshot ring in `CYGTransformSync3D`. Storing into a slot already held by another
## tick silently evicts that occupant; a state that has aged out of the window is
## simply gone. `get_state()` returning null is the contract's signal that a
## reconcile must resimulate from the oldest surviving tick instead of the requested
## one.
##
## States are opaque Variants and are never inspected here. `push()` may arrive out
## of order (an authority retransmit); the newest/oldest marks are re-derived on
## demand so an out-of-order write cannot corrupt them.
##
## Design derives from Netfox's MIT-licensed rollback engine (attribution in
## LICENSE-NETFOX.md). Clean-room GDScript; no Netfox source text is copied.

# Pre-sized parallel arrays: no inner-class typing and no per-frame allocation,
# mirroring the transform-sync ring the rest of the rollback layer already trusts.
var _ticks: Array[int] = []
var _states: Array[Variant] = []
var _used: Array[bool] = []

var _cap: int = 0
var _mask: int = 0
var _count: int = 0

# Cached extremes. `_bounds_valid` goes false whenever an eviction or an erase
# removes a tick the cache was pointing at; the next query re-derives them in one
# allocation-free pass, so the hot path (`push`/`get_state`) never rescans.
var _oldest: int = -1
var _newest: int = -1
var _bounds_valid: bool = true


func _init(capacity: int = 128) -> void:
	# Round up to the next power of two so `tick & mask` is a lossless slot map.
	var target: int = maxi(capacity, 1)
	var slots: int = 1
	while slots < target:
		slots = slots << 1
	_cap = slots
	_mask = _cap - 1
	_ticks.resize(_cap)
	_states.resize(_cap)
	_used.resize(_cap)


## Stores `state` for `value_tick`, overwriting whatever occupied that slot. Keeps
## the oldest/newest marks coherent even when ticks arrive out of order.
func push(value_tick: int, state: Variant) -> void:
	var slot: int = value_tick & _mask
	var was_used: bool = _used[slot]
	if was_used:
		var evicted: int = _ticks[slot]
		if _bounds_valid and (evicted == _oldest or evicted == _newest):
			_bounds_valid = false
	else:
		_count += 1

	_ticks[slot] = value_tick
	_states[slot] = state
	_used[slot] = true

	if _bounds_valid:
		if value_tick > _newest:
			_newest = value_tick
		if _oldest < 0 or value_tick < _oldest:
			_oldest = value_tick


## The state stored for `value_tick`, or null if it was never stored or has aged out
## of the window (slot reused by a different tick).
func get_state(value_tick: int) -> Variant:
	var slot: int = value_tick & _mask
	if _used[slot] and _ticks[slot] == value_tick:
		return _states[slot]
	return null


## The highest tick still present, or -1 when empty.
func newest_tick() -> int:
	_ensure_bounds()
	return _newest


## The lowest tick still present, or -1 when empty.
func oldest_tick() -> int:
	_ensure_bounds()
	return _oldest


## Whether a live state exists for `value_tick` (its slot still names this tick).
func contains(value_tick: int) -> bool:
	var slot: int = value_tick & _mask
	return _used[slot] and _ticks[slot] == value_tick


## Drops every entry whose tick is at or below `value_tick`.
func erase_through(value_tick: int) -> void:
	var removed := false
	for i in range(_cap):
		if _used[i] and _ticks[i] <= value_tick:
			_used[i] = false
			_states[i] = null
			_count -= 1
			removed = true
	if removed:
		_bounds_valid = false


## Keeps only the newest `count` entries; anything older is erased.
func trim_to(count: int) -> void:
	if count >= _count:
		return
	if count <= 0:
		clear()
		return
	# Boundary is the (_count - count)-th smallest tick; erase_through drops it and
	# every smaller tick, leaving exactly the newest `count`.
	var present: Array[int] = keys()
	var boundary: int = present[_count - count - 1]
	erase_through(boundary)


## Number of live entries.
func size() -> int:
	return _count


## Ring capacity (the power-of-two slot count).
func capacity() -> int:
	return _cap


## The oldest tick that survived, i.e. the lowest tick still present; queries for any
## earlier tick are guaranteed to miss. -1 when empty.
func evicted_older_than() -> int:
	return oldest_tick()


## Ascending ticks still present. Allocates the returned array (reconcile-time use).
func keys() -> Array[int]:
	var result: Array[int] = []
	for i in range(_cap):
		if _used[i]:
			result.append(_ticks[i])
	result.sort()
	return result


## Empties the buffer, releasing every stored state.
func clear() -> void:
	for i in range(_cap):
		_used[i] = false
		_states[i] = null
	_count = 0
	_oldest = -1
	_newest = -1
	_bounds_valid = true


## Re-derives the oldest/newest cache in one pass when it is stale. No allocation:
## only reads the typed tick array and updates two integers.
func _ensure_bounds() -> void:
	if _bounds_valid:
		return
	_oldest = -1
	_newest = -1
	for i in range(_cap):
		if _used[i]:
			var t: int = _ticks[i]
			if _oldest < 0 or t < _oldest:
				_oldest = t
			if _newest < 0 or t > _newest:
				_newest = t
	_bounds_valid = true
