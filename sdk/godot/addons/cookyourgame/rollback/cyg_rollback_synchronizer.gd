class_name CYGRollbackSynchronizer
extends RefCounted
## Prediction ledger for peer-hosted rollback: per-object history, pending inputs
## and resimulation.
##
## WHY the shape of this thing: the Rust room relay forwards CustomRpc,
## Spawn/Despawn and top-level state patches but DROPS op 4 (PlayerInput) and
## op 7 (RewindHit), so no server ever rolls anything back. Prediction is
## therefore peer-hosted: the view's owner is the authority for its own state
## (`CYGView.has_authority`), and a non-owner ships its intent to that authority
## as an RPC, then replays the local history recorded here when the authority
## answers. This class is that replay ledger and deliberately knows nothing about
## the wire, the view or the object it drives.
##
## One instance serves several predicted objects keyed by an int id (the view id).
## Derived from netfox's rollback history/prediction flow (MIT, Gálffy Tamás;
## sdk/godot/addons/cookyourgame/LICENSE-NETFOX.md), written clean-room.

## Raised when the ledger rewound and replayed at least one tick. `from_tick` is
## the authoritative tick actually applied, `to_tick` the newest local tick.
signal reconciled(object_id: int, from_tick: int, to_tick: int, mispredicted: int)

## ~2 s at 30 Hz; also the cap on inputs awaiting acknowledgement.
const DEFAULT_HISTORY_WINDOW := 64

## Clock the ledger reads the current local tick from, handed in by the owner.
## Spelled `clock` because naming it `time` would collide with the `_init(time)`
## parameter the contract fixes, and a member shadowed by its own parameter warns.
## When null (a headless or unit-test use with no clock injected), `predict()`
## advances an internal monotonic counter instead so ticks stay increasing.
var clock: CYGNetworkTime = null
## Ticks of history kept per object; read when an object registers.
var history_window: int = DEFAULT_HISTORY_WINDOW

var _slots: Dictionary = {}  # object_id -> _Slot
var _warned_missing: Dictionary = {}  # object_id -> true
var _fallback_tick := 0


func _init(time: CYGNetworkTime) -> void:
	clock = time


## Registers an object. `save()` returns an opaque state snapshot,
## `restore(state)` writes one back, `simulate(input, value_tick)` advances the
## object by one tick. The restored state is always the same opaque shape `save()`
## produced, so a game is free to use a Dictionary, an Array or a custom Resource.
##
## The contract's `load` callback is spelled `restore` here: `load` is a GDScript
## global function and a parameter of that name shadows it with a warning.
func register(object_id: int, save: Callable, restore: Callable, simulate: Callable) -> void:
	if save.is_null() or restore.is_null() or simulate.is_null():
		push_error("CYGRollbackSynchronizer: object %d needs save/restore/simulate callables." % object_id)
		return
	var slot := _Slot.new()
	slot.configure(history_window)
	slot.save = save
	slot.restore = restore
	slot.simulate = simulate
	_slots[object_id] = slot
	_warned_missing.erase(object_id)


func unregister(object_id: int) -> void:
	_slots.erase(object_id)
	_warned_missing.erase(object_id)


func is_registered(object_id: int) -> bool:
	return _slots.has(object_id)


## Records one local prediction: the pre-state goes into the object's history and
## the input goes onto the pending ring, then the simulation runs it immediately.
## Unknown ids are ignored with a single warning per id, never per frame.
func predict(object_id: int, input: Variant) -> void:
	var slot: _Slot = _slots.get(object_id)
	if slot == null:
		if not _warned_missing.has(object_id):
			_warned_missing[object_id] = true
			push_warning("CYGRollbackSynchronizer: predict() for unregistered object %d was ignored." % object_id)
		return
	var value_tick := _current_tick()
	slot.history.push(value_tick, slot.save.call())
	slot.push_input(value_tick, input)
	slot.simulate.call(input, value_tick)


## Tells the ledger everything up to and including `authority_tick` is settled, so
## its history and pending inputs can be released.
func acknowledge(object_id: int, authority_tick: int) -> void:
	var slot: _Slot = _slots.get(object_id)
	if slot == null:
		return
	slot.history.erase_through(authority_tick)
	slot.drop_inputs_through(authority_tick)
	slot.last_authority_tick = maxi(slot.last_authority_tick, authority_tick)


## Loads an authoritative snapshot and replays every stored input newer than
## `authority_tick`, ending on the current tick. Returns the number of ticks the
## correction accounted for: 0 when the prediction already agreed.
func reconcile(object_id: int, authority_tick: int, state: Variant) -> int:
	var slot: _Slot = _slots.get(object_id)
	if slot == null:
		return 0

	var predicted: Variant = slot.save.call()
	var effective := authority_tick
	var lost := 0
	var oldest: int = slot.history.oldest_tick()
	# WHY: the authority can answer from a tick our window already evicted under
	# lag or a stall. Rewinding to a state we no longer hold is impossible, so the
	# oldest snapshot available is used and the missing span is charged to the
	# misprediction count rather than dropped on the floor.
	if oldest >= 0 and authority_tick < oldest:
		lost = oldest - authority_tick
		effective = oldest

	slot.history.erase_through(effective)
	slot.history.push(effective, state)
	slot.restore.call(state)

	var replayed := 0
	var to_tick := effective
	var capacity := slot.input_ticks.size()
	for i in range(slot.count):
		var at := (slot.head + i) % capacity
		var value_tick: int = slot.input_ticks[at]
		if value_tick <= effective:
			continue
		slot.history.push(value_tick, slot.save.call())
		slot.simulate.call(slot.input_values[at], value_tick)
		replayed += 1
		to_tick = value_tick

	slot.drop_inputs_through(effective)
	slot.last_authority_tick = maxi(slot.last_authority_tick, authority_tick)

	# Resimulation lands back on the tick we were already on, so comparing the
	# settled state with the state we predicted says whether the authority agreed.
	# `!=` is exact and therefore conservative: a state that came back through a
	# lossy codec (a packed Vector3, say) never compares equal, and the whole
	# replay is then reported as a correction.
	var settled: Variant = slot.save.call()
	var corrected := 0
	if (replayed > 0 or lost > 0) and predicted != settled:
		corrected = replayed + lost
	slot.mispredicted = corrected
	if corrected > 0 or replayed > 0:
		reconciled.emit(object_id, effective, to_tick, corrected)
	return corrected


## Ticks the last `reconcile()` accounted for.
func misprediction_count(object_id: int) -> int:
	var slot: _Slot = _slots.get(object_id)
	return slot.mispredicted if slot != null else 0


## Highest authority tick seen for the object, -1 when none.
func last_authority_tick(object_id: int) -> int:
	var slot: _Slot = _slots.get(object_id)
	return slot.last_authority_tick if slot != null else -1


## Inputs recorded but not yet consumed by an authority.
func pending_input_count(object_id: int) -> int:
	var slot: _Slot = _slots.get(object_id)
	return slot.count if slot != null else 0


## Empties every ledger but keeps the registrations, for a room leave/rejoin.
func reset() -> void:
	_fallback_tick = 0
	_warned_missing.clear()
	for slot_id in _slots.keys():
		var slot: _Slot = _slots[slot_id]
		slot.reset()


func _current_tick() -> int:
	if clock != null:
		return clock.tick()
	_fallback_tick += 1
	return _fallback_tick


## One object's history ring plus its pending inputs.
class _Slot:
	extends RefCounted

	var save: Callable
	var restore: Callable
	var simulate: Callable
	var history: CYGHistoryBuffer
	var input_ticks: Array[int] = []
	## Variants, so untyped by necessity; released to null once consumed.
	var input_values: Array = []
	var head := 0
	var count := 0
	var last_authority_tick := -1
	var mispredicted := 0

	func configure(window: int) -> void:
		var capacity := maxi(window, 2)
		history = CYGHistoryBuffer.new(capacity)
		input_ticks.resize(capacity)
		input_values.resize(capacity)
		head = 0
		count = 0

	## Writes into the ring, discarding the oldest input when a stalled authority
	## has left it full: bounded memory outranks replaying an ancient intent.
	func push_input(value_tick: int, input: Variant) -> void:
		var capacity := input_ticks.size()
		if count == capacity:
			input_values[head] = null
			head = (head + 1) % capacity
			count -= 1
		var at := (head + count) % capacity
		input_ticks[at] = value_tick
		input_values[at] = input
		count += 1

	func drop_inputs_through(value_tick: int) -> void:
		var capacity := input_ticks.size()
		while count > 0 and input_ticks[head] <= value_tick:
			input_values[head] = null
			head = (head + 1) % capacity
			count -= 1

	func reset() -> void:
		history.clear()
		for i in range(input_values.size()):
			input_values[i] = null
		head = 0
		count = 0
		last_authority_tick = -1
		mispredicted = 0
