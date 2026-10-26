class_name CYGRollbackPredictor
extends RefCounted
## GGPO-style predict / reconcile ledger, ported from
## `CYGRollbackPredictor` in the JS SDK.
##
## ```gdscript
## var rollback := CYGRollbackPredictor.new(64)
## var predicted := rollback.predict(input, func(state, input): ...)
## var fixed := rollback.on_server_ack(tick, server_state, resimulate)
## ```

var max_history := 64
var ring_buffer: Array = []
var pending_inputs: Array = []
var current_tick := 0
var last_server_tick := 0
var current_state: Variant = null


func _init(history: int = 64) -> void:
	max_history = maxi(history, 1)
	ring_buffer.resize(max_history)
	ring_buffer.fill(null)


## Instant local prediction for the next tick without waiting for the server.
func predict(input: Variant, step_fn: Callable = Callable()) -> Dictionary:
	current_tick += 1
	pending_inputs.append({"tick": current_tick, "input": input})
	if current_state != null and step_fn.is_valid():
		current_state = step_fn.call(current_state, input)
	var slot := current_tick % max_history
	ring_buffer[slot] = {
		"tick": current_tick,
		"input": input,
		"state": _deep_copy(current_state),
	}
	return {"tick": current_tick, "state": current_state}


## Reconcile against an authoritative frame; replays every unacknowledged input.
func on_server_ack(server_tick: int, authoritative_state: Variant,
		resimulate_fn: Callable = Callable()) -> Dictionary:
	last_server_tick = server_tick
	var remaining: Array = []
	for pending in pending_inputs:
		if int(pending["tick"]) > server_tick:
			remaining.append(pending)
	pending_inputs = remaining

	var slot := server_tick % max_history
	var historical: Variant = ring_buffer[slot]
	var needs_rollback := true
	if historical != null and int(historical["tick"]) == server_tick and historical["state"] != null:
		if _states_equal(historical["state"], authoritative_state):
			needs_rollback = false

	if not needs_rollback:
		return {"rolledBack": false, "state": current_state, "replayedTicks": 0}

	var reconciled: Variant = _deep_copy(authoritative_state)
	if resimulate_fn.is_valid():
		for pending in pending_inputs:
			reconciled = resimulate_fn.call(reconciled, pending["input"])
			var pending_slot := int(pending["tick"]) % max_history
			if ring_buffer[pending_slot] != null:
				ring_buffer[pending_slot]["state"] = _deep_copy(reconciled)
	current_state = reconciled
	return {"rolledBack": true, "state": current_state, "replayedTicks": pending_inputs.size()}


static func _deep_copy(value: Variant) -> Variant:
	if typeof(value) == TYPE_DICTIONARY or typeof(value) == TYPE_ARRAY:
		return value.duplicate(true)
	return value


static func _states_equal(a: Variant, b: Variant) -> bool:
	var type_a := typeof(a)
	if type_a != typeof(b):
		return false
	match type_a:
		TYPE_DICTIONARY:
			var da: Dictionary = a
			var db: Dictionary = b
			if da.size() != db.size():
				return false
			for key in da.keys():
				if not db.has(key) or not _states_equal(da[key], db[key]):
					return false
			return true
		TYPE_ARRAY, TYPE_PACKED_FLOAT64_ARRAY, TYPE_PACKED_FLOAT32_ARRAY, TYPE_PACKED_INT32_ARRAY:
			if a.size() != b.size():
				return false
			for i in range(a.size()):
				if not _states_equal(a[i], b[i]):
					return false
			return true
		_:
			return a == b
