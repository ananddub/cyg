class_name CYGPredictiveSync
extends CYGBehaviour
## Client-side prediction for a spawned-everywhere object whose truth lives on
## another peer (a projectile, a dash, an ability's VFX).
##
## This component syncs NO properties: it never calls `register_as_observable()`,
## so it never enters the view's dirty mask, and `has_delta()` is always false.
## Every byte it moves is an RPC. Locally it runs `move` on the spot, ships the
## intent to the authority, and on the authority's answer rewinds its ledger and
## replays the inputs recorded since, via `CYGRollbackSynchronizer`.
##
## WHY an RPC round trip rather than server input prediction: the Rust room relay
## forwards CustomRpc but DROPS op 4 (PlayerInput) and op 7 (RewindHit), so no
## server-side input handler exists to rewind. The only peer that can adjudicate
## an intent is the one `has_authority` names for that view, and RPC is the only
## channel that reaches it. Consequence: the authority answers in its own tick
## space, so a clock this component created itself is locked to that space with
## `CYGNetworkTime.observe_tick()`; an injected room clock is left alone, since its
## owner keeps it aligned for every predictor at once.
##
## One instance per networked object: the ledger keys predictions by view id and
## `cyg_predict` binds one handler per view.
##
## Wire layout, each RPC carrying exactly one `PackedByteArray` argument:
## `cyg_predict`  `[tick u32 BE][requester actor u32 BE][intent value]`
## `<confirm_name>` `[tick u32 BE][requester actor u32 BE][state value]`
## A "value" is one tagged field (see `_write_value`); the built-in snapshot is
## `[Vector3 position, float speed]`, so a projectile costs 6 + 4 + tag bytes.
##
## Derived from netfox's rollback prediction flow (MIT, Gálffy Tamás;
## sdk/godot/addons/cookyourgame/LICENSE-NETFOX.md), written clean-room.

const RPC_PREDICT := "cyg_predict"
const RPC_CONFIRM_DEFAULT := "cyg_confirm"

# Tag bytes for the self-describing value stream inside a blob. Vector3 rides as
# the 6-byte int16x100 packing and floats as `write_single`, the widths a
# predicted projectile actually wants rather than the router's 12-byte default.
const KIND_VOID := 0
const KIND_BOOL := 1
const KIND_INT := 2
const KIND_FLOAT := 3
const KIND_STRING := 4
const KIND_VECTOR2 := 5
const KIND_VECTOR3 := 6
const KIND_QUATERNION := 7
const KIND_BYTES := 8
const KIND_ARRAY := 9
const KIND_DICTIONARY := 10
## Cap on a decoded container: a corrupt count must not allocate unbounded.
const MAX_CONTAINER_FIELDS := 64

## Who owns the truth for this object; a `CYGOpcodes.Target` value.
@export var authority_target: int = CYGOpcodes.Target.MASTER_CLIENT
## Longest rewind worth replaying; anything older snaps to the authority instead.
@export var max_rollback_ticks: int = 30
## Cumulative corrected ticks that make the ledger noisy enough to be worth a
## warning (a game that simulates too aggressively trips this, not the network).
@export var mispredict_warn_ticks: int = 8

## Clock the ledger numbers its ticks with. The owner injects the room's shared
## `CYGNetworkTime`; when null this component creates and drives its own.
var time: CYGNetworkTime = null
## Scalar the built-in snapshot carries next to the parent's position: the game's
## `move` writes it (a projectile's velocity magnitude) and a restore reads it
## back. Replace the whole snapshot with `configure_state()` if it does not fit.
var sim_speed: float = 0.0

var _move: Callable = Callable()
var _confirm_name := RPC_CONFIRM_DEFAULT
var _save_state: Callable = Callable()
var _restore_state: Callable = Callable()
var _sync: CYGRollbackSynchronizer = null
var _owns_time := false
## False until the first `cyg_confirm` places this peer's ticks in the authority's
## numbering; predictions made before that cannot be reconciled against it.
var _anchored := false
var _node_3d: Node3D = null
var _ticked := false

var _warned_no_view := false
var _warned_no_move := false
var _warned_impossible := false
var _warned_mispredicting := false
var _mispredict_streak := 0
var _last_intent_tick: Dictionary = {}  # requester actor -> newest intent tick


## Installs the local integrator, run as `move(input, delta_ms)`, and names the
## RPC the authority confirms on. The confirm name must be unique per predictive
## component sharing a view; `cyg_predict` is fixed.
func configure(move: Callable, confirm_name: String = RPC_CONFIRM_DEFAULT) -> void:
	_move = move
	_confirm_name = confirm_name if not confirm_name.is_empty() else RPC_CONFIRM_DEFAULT


## Overrides the opaque snapshot the ledger stores and the confirm ships, as
## `save() -> Variant` and `restore(state)`. Both peers must be configured the
## same way or the decoded state will not fit.
func configure_state(save: Callable, restore: Callable) -> void:
	_save_state = save
	_restore_state = restore


func _ready() -> void:
	_node_3d = get_parent() as Node3D
	if time == null:
		time = CYGNetwork.network_time
	if time == null:
		time = CYGNetworkTime.new()
		time.ticks_per_second = _fallback_tick_rate()
		_owns_time = true
	if not CYGNetwork.network_tick.is_connected(_on_network_tick):
		CYGNetwork.network_tick.connect(_on_network_tick)
	_ticked = true


func _exit_tree() -> void:
	if _ticked and is_instance_valid(CYGNetwork):
		CYGNetwork.network_tick.disconnect(_on_network_tick)
	_ticked = false
	if _sync != null:
		var object_id := _object_id()
		if object_id >= 0:
			_sync.unregister(object_id)
	super._exit_tree()


func cyg_on_spawned() -> void:
	var target := view
	if target == null:
		return
	target.bind_rpc(RPC_PREDICT, _on_predict_rpc)
	target.bind_rpc(_confirm_name, _on_confirm_rpc)
	_last_intent_tick.clear()
	_mispredict_streak = 0
	_warned_impossible = false
	_warned_mispredicting = false
	_anchored = false
	_ensure_registered()


func cyg_on_despawned() -> void:
	if _sync != null:
		var object_id := _object_id()
		if object_id >= 0:
			_sync.unregister(object_id)


## The truth moved to another peer, so the tick space did: forget the outstanding
## predictions rather than reconcile them against a stranger's history.
func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	_reset_ledger()
	_mispredict_streak = 0
	_warned_impossible = false
	_warned_mispredicting = false
	_anchored = false


## Predicts `input` on the spot and ships it to the authority as one RPC.
func predict_input(input: Variant) -> void:
	if _move.is_null():
		if not _warned_no_move:
			_warned_no_move = true
			push_warning("CYGPredictiveSync on %s: configure() was never called; predict_input() is a no-op." % name)
		return
	var object_id := _object_id()
	if object_id < 0:
		return
	_ensure_registered()

	var value_tick := _current_tick()
	# The ledger's `simulate` hook runs `move`, so the local step happens here and
	# is never run twice.
	_sync.predict(object_id, input)

	if _is_truth_peer():
		# Our own state is the definition of correct: settle it immediately so the
		# history ring does not fill with snapshots nothing will ever rewind to.
		_sync.acknowledge(object_id, value_tick)
		return

	var blob := _build_blob(value_tick, _local_actor(), input)
	send_rpc(RPC_PREDICT, [blob], authority_target)


## Rolls the local ledger back to `authority_tick` and replays forward, or snaps
## when the gap is too wide to resimulate honestly.
func apply_authority(authority_tick: int, state: Variant) -> void:
	var object_id := _object_id()
	if object_id < 0:
		return
	_ensure_registered()

	# WHY measured first: re-anchoring the clock hides the gap this check exists to
	# find, so the rewind is taken against the old numbering.
	var rewind: int = _current_tick() - authority_tick
	if not _anchored or rewind > max_rollback_ticks:
		if rewind > max_rollback_ticks and not _warned_impossible:
			_warned_impossible = true
			push_warning("CYGPredictiveSync on %s: authority tick %d is %d ticks behind (max_rollback_ticks=%d); snapping to the authoritative state."
				% [name, authority_tick, rewind, max_rollback_ticks])
		# The first answer, and any gap wider than the window, is adopted outright:
		# predictions recorded under a different tick numbering cannot honestly be
		# compared with this one, so the ledger restarts instead of replaying them.
		# That first adoption doubles as the object's initial state, the only one this
		# component has: it syncs no properties and rides no spawn payload.
		_reset_ledger()
		_write_state(state)
		_mispredict_streak = 0
		_observe(authority_tick)
		_anchored = true
		return

	# WHY: the authority confirmed in its tick space, so the next intent must be
	# recorded under a tick comparable with the one just reconciled against.
	_observe(authority_tick)
	var mispredicted: int = _sync.reconcile(object_id, authority_tick, state)
	_warned_impossible = false
	if mispredicted <= 0:
		_mispredict_streak = 0
		_warned_mispredicting = false
		return
	_mispredict_streak += mispredicted
	if _mispredict_streak >= mispredict_warn_ticks and not _warned_mispredicting:
		_warned_mispredicting = true
		push_warning("CYGPredictiveSync on %s: %d ticks corrected in a row, prediction is not converging."
			% [name, _mispredict_streak])


## This component owns no properties, so the view's dirty mask never selects it
## and there is never a state delta to send.
func has_delta() -> bool:
	return false


## Writes nothing beyond the base property block, which is itself empty because
## this component registers no properties; kept as an override so the pair stays
## byte-symmetric should a subclass ever become observable.
func serialize(writer: CYGBitPacker) -> void:
	super.serialize(writer)


func deserialize(reader: CYGBitReader) -> void:
	super.deserialize(reader)


## ------------------------------------------------------------------- internals

func _on_network_tick(_tick: int) -> void:
	if time == null:
		return
	if is_instance_valid(CYGNetwork):
		time.latency_ms = CYGNetwork.latency_ms
	# Only a clock we created is ours to advance; an injected room clock is driven
	# by its owner, and advancing it twice would double the tick rate.
	if _owns_time:
		time.advance(_step_ms())
	if _mispredict_streak > 0:
		# Decay so one bad burst warns once instead of latching forever.
		_mispredict_streak -= 1


func _on_predict_rpc(args: Array) -> void:
	# MASTER_CLIENT and OBSERVERS reach this peer as a relay broadcast, so every
	# listener decodes this: only the peer the view's truth lives on may act.
	if not _is_truth_peer():
		return
	var blob := _blob_from_args(args, RPC_PREDICT)
	if blob.is_empty():
		return

	var reader := CYGBitReader.new()
	reader.attach(blob)
	var intent_tick: int = reader.read_uint32_be()
	var requester: int = reader.read_uint32_be()
	if reader.last_read_failed:
		return
	# The relay offers no ordering guarantee, and a resent intent would integrate
	# the same step twice. -1 is below every wire tick, since ticks are unsigned and
	# a session legitimately starts at 0.
	var previous: int = _last_intent_tick.get(requester, -1)
	if intent_tick <= previous:
		return
	_last_intent_tick[requester] = intent_tick

	var intent: Variant = _read_value(reader)
	_simulate_step(intent, intent_tick)
	# The reply addresses the requester by actor because the relay can only
	# unicast to a view's owner or broadcast to everyone else.
	_send_confirm(requester, _current_tick(), _capture_state())


func _on_confirm_rpc(args: Array) -> void:
	var blob := _blob_from_args(args, _confirm_name)
	if blob.is_empty():
		return

	var reader := CYGBitReader.new()
	reader.attach(blob)
	var authority_tick: int = reader.read_uint32_be()
	var requester: int = reader.read_uint32_be()
	if reader.last_read_failed:
		return
	# A confirm broadcast by an authority that could not unicast reaches the whole
	# room; only the peer that asked decodes further.
	if requester != _local_actor():
		return
	var state: Variant = _read_value(reader)
	apply_authority(authority_tick, state)


func _send_confirm(requester: int, authority_tick: int, state: Variant) -> void:
	if requester == _local_actor():
		return
	var blob := _build_blob(authority_tick, requester, state)
	var target := view
	if target != null and requester == target.owner_actor:
		# The relay unicasts target byte 3 to the view's spawner, which is exactly
		# the peer waiting for this answer.
		send_rpc(_confirm_name, [blob], CYGOpcodes.Target.OWNER)
		return
	# No dedicated path exists for an arbitrary actor, so it goes out as a
	# multicast the other peers discard on the requester check above.
	send_rpc(_confirm_name, [blob], CYGOpcodes.Target.ALL)


func _ensure_registered() -> void:
	var object_id := _object_id()
	if object_id < 0:
		return
	if _sync == null:
		_sync = CYGRollbackSynchronizer.new(time)
	elif _sync.clock != time:
		# An owner may hand over the room clock after spawn; the ledger has to count
		# with the same one this component reads or the ticks stop lining up.
		_sync.clock = time
	if _sync.is_registered(object_id):
		return
	_sync.register(object_id, _capture_state, _write_state, _simulate_step)


func _reset_ledger() -> void:
	if _sync == null:
		return
	var object_id := _object_id()
	if object_id >= 0:
		_sync.unregister(object_id)
	_last_intent_tick.clear()


func _object_id() -> int:
	var target := view
	if target == null:
		if not _warned_no_view:
			_warned_no_view = true
			push_warning("CYGPredictiveSync on %s: no CYGView in the parent chain; prediction is offline." % name)
		return -1
	return target.view_id


## True when this peer is the one whose state the others reconcile against.
func _is_truth_peer() -> bool:
	match authority_target:
		CYGOpcodes.Target.OWNER:
			return is_mine
		CYGOpcodes.Target.MASTER_CLIENT:
			return is_instance_valid(CYGNetwork) and CYGNetwork.is_master_client
		_:
			# A broadcast authority has no single adjudicator, so the view's own
			# owner settles its state and everybody else rolls back to it.
			return has_authority


func _current_tick() -> int:
	if time != null:
		return time.tick()
	if not is_instance_valid(CYGNetwork):
		return 0
	return CYGNetwork.current_tick


func _local_actor() -> int:
	if not is_instance_valid(CYGNetwork):
		return 0
	return CYGNetwork.local_actor_number


func _step_ms() -> float:
	if time != null:
		return time.tick_duration_ms()
	return 1000.0 / float(maxi(_fallback_tick_rate(), 1))


func _fallback_tick_rate() -> int:
	if not is_instance_valid(CYGNetwork):
		return 30
	return CYGNetwork.tick_rate


## WHY the fixed tick step rather than a render delta: resimulation replays whole
## ticks, so `move` must see the same dt during a rollback that it saw live. The
## tick itself is ignored: `CYGRollbackSynchronizer` hands every input the same
## two-argument shape, live and replayed alike.
func _simulate_step(input: Variant, _value_tick: int) -> void:
	if _move.is_null():
		return
	_move.call(input, _step_ms())


func _observe(authority_tick: int) -> void:
	# WHY gated on ownership: an injected clock is the room's shared timeline, kept
	# aligned by whoever owns it; re-anchoring it from one object's answer would drag
	# every other predictor's ticks with it. Only a clock this component made is
	# private enough to lock onto this authority. A negative tick can only come from a
	# corrupt packet, so it is never allowed to move the timeline either.
	if _owns_time and time != null and authority_tick >= 0:
		time.observe_tick(authority_tick)


func _capture_state() -> Variant:
	if not _save_state.is_null():
		return _save_state.call()
	return [_read_position(), sim_speed]


func _write_state(state: Variant) -> void:
	if not _restore_state.is_null():
		_restore_state.call(state)
		return
	if not (state is Array):
		return
	var snapshot: Array = state
	if snapshot.size() < 2:
		return
	if _node_3d != null and is_instance_valid(_node_3d) and snapshot[0] is Vector3:
		_node_3d.global_position = snapshot[0]
	if snapshot[1] is float or snapshot[1] is int:
		sim_speed = snapshot[1]


func _read_position() -> Vector3:
	if _node_3d != null and is_instance_valid(_node_3d):
		return _node_3d.global_position
	return Vector3()


## ------------------------------------------------------------------ blob codec

func _build_blob(value_tick: int, requester_actor: int, value: Variant) -> PackedByteArray:
	var writer := CYGBitPacker.new()
	# Ticks ride unsigned 32-bit: a room cannot outlive four billion ticks.
	writer.write_uint32_be(value_tick)
	writer.write_uint32_be(requester_actor)
	_write_value(writer, value)
	return writer.to_bytes()


func _blob_from_args(args: Array, rpc_name: String) -> PackedByteArray:
	if args.is_empty():
		push_warning("CYGPredictiveSync: '%s' arrived with no argument." % rpc_name)
		return PackedByteArray()
	var first: Variant = args[0]
	if not (first is PackedByteArray):
		push_warning("CYGPredictiveSync: '%s' expected a PackedByteArray blob, got type %d." % [rpc_name, typeof(first)])
		return PackedByteArray()
	var blob: PackedByteArray = first
	return blob


func _write_value(writer: CYGBitPacker, value: Variant) -> void:
	match typeof(value):
		TYPE_BOOL:
			writer.write_byte(KIND_BOOL)
			writer.write_bool(value)
		TYPE_INT:
			writer.write_byte(KIND_INT)
			writer.write_int32(value)
		TYPE_FLOAT:
			writer.write_byte(KIND_FLOAT)
			writer.write_single(value)
		TYPE_STRING:
			writer.write_byte(KIND_STRING)
			writer.write_string(value)
		TYPE_VECTOR2:
			writer.write_byte(KIND_VECTOR2)
			writer.write_vector2(value)
		TYPE_VECTOR3:
			writer.write_byte(KIND_VECTOR3)
			writer.write_vector3_packed(value)
		TYPE_QUATERNION:
			writer.write_byte(KIND_QUATERNION)
			writer.write_quaternion(value)
		TYPE_PACKED_BYTE_ARRAY:
			writer.write_byte(KIND_BYTES)
			writer.write_uint32(value.size())
			writer.write_raw(value)
		TYPE_ARRAY:
			writer.write_byte(KIND_ARRAY)
			var items: Array = value
			writer.write_uint32_be(items.size())
			for item in items:
				_write_value(writer, item)
		TYPE_DICTIONARY:
			writer.write_byte(KIND_DICTIONARY)
			var entries: Dictionary = value
			writer.write_uint32_be(entries.size())
			for key in entries.keys():
				_write_value(writer, key)
				_write_value(writer, entries[key])
		TYPE_NIL:
			writer.write_byte(KIND_VOID)
		_:
			# An unencodable shape is tagged void rather than skipped, so the reader
			# stays aligned with everything packed after it.
			writer.write_byte(KIND_VOID)
			push_warning("CYGPredictiveSync: cannot encode value of type %d into an RPC blob." % typeof(value))


func _read_value(reader: CYGBitReader) -> Variant:
	var kind: int = reader.read_byte()
	if reader.last_read_failed:
		return null
	match kind:
		KIND_BOOL:
			return reader.read_bool()
		KIND_INT:
			return reader.read_int32()
		KIND_FLOAT:
			return reader.read_single()
		KIND_STRING:
			return reader.read_string()
		KIND_VECTOR2:
			return reader.read_vector2()
		KIND_VECTOR3:
			return reader.read_vector3_packed()
		KIND_QUATERNION:
			return reader.read_quaternion()
		KIND_BYTES:
			var byte_count: int = reader.read_uint32()
			if byte_count <= 0 or byte_count > reader.remaining:
				push_warning("CYGPredictiveSync: blob asks for %d bytes with %d left." % [byte_count, reader.remaining])
				return PackedByteArray()
			return reader.read_bytes(byte_count)
		KIND_ARRAY:
			var fields: int = reader.read_uint32_be()
			if fields > MAX_CONTAINER_FIELDS:
				push_warning("CYGPredictiveSync: blob claims %d array fields, refusing to decode." % fields)
				return null
			var items: Array = []
			for i in range(fields):
				var item: Variant = _read_value(reader)
				if reader.last_read_failed:
					break
				items.append(item)
			return items
		KIND_DICTIONARY:
			var pairs: int = reader.read_uint32_be()
			if pairs > MAX_CONTAINER_FIELDS:
				push_warning("CYGPredictiveSync: blob claims %d dictionary entries, refusing to decode." % pairs)
				return null
			var entries: Dictionary = {}
			for i in range(pairs):
				var key: Variant = _read_value(reader)
				var value: Variant = _read_value(reader)
				if reader.last_read_failed:
					break
				entries[key] = value
			return entries
		_:
			return null
