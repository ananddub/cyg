class_name CYGCharacterSync2D
extends CYGBehaviour
## Networked motion for a `CharacterBody2D`: velocity, grounding and the
## animation-facing speed. The pose stays with `CYGTransformSync2D`.
##
## WHY two modes: the Rust room relay forwards CustomRpc, Spawn/Despawn and state
## patches but DROPS op 4 (PlayerInput) and op 7 (RewindHit), so there is no
## server-side input reserver. Prediction is therefore peer-hosted:
## `OWNER_AUTHORITY` lets the view owner integrate while its sync properties carry
## the result; `REMOTE_AUTHORITY` keeps the view owner as the integrator but ships
## tick-stamped snapshots plus the input stream, so every other peer predicts the
## authority's next move and rewinds through `CYGRollbackSynchronizer` when wrong.
##
## Never writes position/rotation/scale: `CYGTransformSync2D` owns that stream, and
## a second pose stream would fight its interpolation.
##
## Payload after the base sync-property block: [byte flags], then
## [authority tick u32 BE] when FLAG_AUTHORITY_TICK, then [velocity Vector2] when
## FLAG_VELOCITY, then [grounded bool] when FLAG_GROUNDED. Same flags-byte habit as
## the transform sync, so the two blocks decode independently off one reader.
##
## Prediction shape derives from netfox's network-rollback character tutorial
## (MIT, © Gálffy Tamás); written independently against this SDK's contracts.

enum AuthorityMode { OWNER_AUTHORITY, REMOTE_AUTHORITY }

const FLAG_VELOCITY := 1
const FLAG_GROUNDED := 2
const FLAG_AUTHORITY_TICK := 4

const PROP_VELOCITY := "velocity"
const PROP_GROUNDED := "grounded"
const PROP_ANIMATION_SPEED := "animation_speed"

const RPC_INPUT := "cyg_input"

## Input history is a power-of-two ring so a rewind never allocates.
const INPUT_RING := 64
const INPUT_MASK := INPUT_RING - 1

## WHY: a small nudge is invisible, a large one must not look like rubber-banding.
const CORRECTION_BLEND_SECONDS := 0.12
const CORRECTION_EPSILON := 0.01

## Authority mode of this component; see the docblock for what each one implies.
@export var authority_mode: AuthorityMode = AuthorityMode.OWNER_AUTHORITY
## Horizontal speed the input intent is scaled to.
@export var move_speed: float = 6.0
## Units/s per second used to reach the target horizontal speed.
@export var acceleration: float = 40.0
## Applied on Y while airborne; 2D Y grows downward, so the same negative default
## used in 3D still pulls the body toward the floor.
@export var gravity: float = -19.6
## Launch speed on Y for a jump edge; 2D up is -Y, so it is applied negated.
@export var jump_impulse: float = 8.0
## Publish the horizontal speed as the `animation_speed` sync property.
@export var sync_animation_speed: bool = true
## Corrections faster than this snap instead of blending.
@export var max_correction_speed: float = 20.0

## Rollback clock; defaults to the room's `CYGNetwork.network_time`.
var time: CYGNetworkTime = null
## Movement intent the game writes each frame: x on X, -1..1. `y` is carried on the
## wire for a shared blob layout but vertical intent is the jump edge, so the
## integrator does not consume it.
var input_vector: Vector2 = Vector2()
## Jump edge; the integrator consumes it once per tick.
var jump_pressed: bool = false

var _body: CharacterBody2D = null
var _velocity := Vector2()
var _grounded := false
var _animation_speed := 0.0

var _state_dirty := false
var _out_flags := 0
var _out_tick := 0
var _out_velocity := Vector2()
var _out_grounded := false

var _ring_tick: Array[int] = []
var _ring_input: Array[int] = []
var _remote_tick: Array[int] = []
var _remote_input: Array[int] = []

var _authority_move := Vector2()
var _authority_jump := false
var _authority_input_seen := false

## The authority's tick numbering re-expressed in this peer's ledger: two processes
## count ticks from their own start, so a stamp is meaningless until mapped.
var _authority_tick_offset := 0
var _authority_anchored := false
var _last_authority_tick := -1

var _synchronizer: CYGRollbackSynchronizer = null
var _correcting := false
var _correction_target := Vector2()

var _ticked := false
var _rpcs_bound := false
var _integrate_steps := 0
var _blob_writer: CYGBitPacker = null
var _blob_args: Array = [null]


func _ready() -> void:
	_body = get_parent() as CharacterBody2D
	if _body == null:
		push_error("CYGCharacterSync2D on %s: parent is not a CharacterBody2D; motion will not sync." % name)
		push_warning("CYGCharacterSync2D: place this node under the CharacterBody2D it should drive.")
		return
	_ring_tick.resize(INPUT_RING)
	_ring_input.resize(INPUT_RING)
	_remote_tick.resize(INPUT_RING)
	_remote_input.resize(INPUT_RING)
	for i in range(INPUT_RING):
		_ring_tick[i] = -1
		_ring_input[i] = 0
		_remote_tick[i] = -1
		_remote_input[i] = 0
	_blob_writer = CYGBitPacker.new()
	# Default to the room clock: private clocks diverge between the authority and its
	# proxies, and a proxy must rewind to the tick the authority stamped.
	if time == null:
		time = CYGNetwork.network_time
	# Registration order is the wire identity, so the property set is fixed here and
	# `sync_animation_speed` only gates writes: peers with differing export values
	# must still agree on every property index.
	register_property(PROP_VELOCITY, Vector2(), _on_remote_property)
	register_property(PROP_GROUNDED, false, _on_remote_property)
	register_property(PROP_ANIMATION_SPEED, 0.0, _on_remote_property)
	CYGNetwork.network_tick.connect(_on_network_tick)
	_ticked = true


func _exit_tree() -> void:
	if _ticked:
		CYGNetwork.network_tick.disconnect(_on_network_tick)
		_ticked = false
	if _synchronizer != null:
		_synchronizer.unregister(_object_id())
	super._exit_tree()


func cyg_on_spawned() -> void:
	_velocity = Vector2()
	_grounded = false
	_animation_speed = 0.0
	_state_dirty = false
	_out_flags = 0
	_out_tick = 0
	_out_velocity = Vector2()
	_out_grounded = false
	_correcting = false
	_reset_authority_space()
	_clear_history()


func cyg_on_despawned() -> void:
	_state_dirty = false
	_out_flags = 0
	_correcting = false
	_reset_authority_space()
	_clear_history()
	if _synchronizer != null:
		_synchronizer.unregister(_object_id())
		_synchronizer = null


## The peer doing the integrating just changed, so every prediction held here is
## about to be wrong.
func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	_correcting = false
	_reset_authority_space()
	_clear_history()
	if _synchronizer != null:
		_synchronizer.reset()


## Feeds this tick's intent; `jump` is edge-triggered and consumed by the integrator.
func set_input(move: Vector2, jump: bool) -> void:
	input_vector = move
	if jump:
		jump_pressed = true


var velocity: Vector2:
	get:
		return _velocity


var grounded: bool:
	get:
		return _grounded


## Horizontal speed: the animation-facing value the `animation_speed` property carries.
var animation_speed: float:
	get:
		return _animation_speed


## The clock this component reads ticks from. `_ready` adopts the room clock, so a
## component never predicts on a timeline of its own; this only builds a private one
## when the singleton is not there to supply it (a scene assembled by a unit test).
func network_time() -> CYGNetworkTime:
	if time == null:
		time = CYGNetworkTime.new()
		time.ticks_per_second = CYGNetwork.tick_rate
	return time


func has_delta() -> bool:
	var base_dirty := super.has_delta()
	return _state_dirty or base_dirty


## Ticks the last reconciliation re-simulated. Non-zero on a proxy proves the
## rollback path corrected its prediction instead of merely adopting a value.
func misprediction_count() -> int:
	return _synchronizer.misprediction_count(_object_id()) if _synchronizer != null else 0


## Inputs recorded but not yet settled by the authority.
func pending_input_count() -> int:
	return _synchronizer.pending_input_count(_object_id()) if _synchronizer != null else 0


func serialize(writer: CYGBitPacker) -> void:
	super.serialize(writer)
	# The flags byte is written even when zero so deserialize's fixed layout stays
	# symmetric with property-only deltas.
	writer.write_byte(_out_flags)
	if not _state_dirty:
		return

	if (_out_flags & FLAG_AUTHORITY_TICK) != 0:
		writer.write_uint32_be(_out_tick)
	if (_out_flags & FLAG_VELOCITY) != 0:
		writer.write_vector2(_out_velocity)
	if (_out_flags & FLAG_GROUNDED) != 0:
		writer.write_bool(_out_grounded)
	_out_flags = 0
	_state_dirty = false


func deserialize(reader: CYGBitReader) -> void:
	super.deserialize(reader)

	var flags := reader.read_byte()
	var has_tick := (flags & FLAG_AUTHORITY_TICK) != 0
	var has_velocity := (flags & FLAG_VELOCITY) != 0
	var has_grounded := (flags & FLAG_GROUNDED) != 0

	# The fixed order is the layout: reading is flag-driven but never reordered.
	var tick := 0
	if has_tick:
		tick = int(reader.read_uint32_be())
	var next_velocity := Vector2()
	if has_velocity:
		next_velocity = reader.read_vector2()
	var next_grounded := false
	if has_grounded:
		next_grounded = reader.read_bool()

	# The authority decoded the block only to keep the shared reader aligned; its own
	# simulation owns velocity, grounding and the pose.
	if has_authority:
		return

	if not has_velocity:
		if has_grounded:
			_grounded = next_grounded
		return

	# A REMOTE_AUTHORITY proxy reconciles against the tick-stamped snapshot; a plain
	# OWNER_AUTHORITY proxy has nothing to rewind and adopts the value directly.
	if authority_mode == AuthorityMode.REMOTE_AUTHORITY and has_tick:
		_reconcile(tick, next_velocity, next_grounded, has_grounded)
	else:
		_velocity = next_velocity
		_animation_speed = _planar_speed(_velocity)
		if has_grounded:
			_grounded = next_grounded


## ------------------------------------------------------------------- tick loop

func _on_network_tick(tick: int) -> void:
	if _body == null or not is_instance_valid(_body):
		return
	if not _ensure_bound():
		return
	if has_authority:
		_authority_tick(tick)
	elif authority_mode == AuthorityMode.REMOTE_AUTHORITY:
		_predict_tick(tick)


## Binds the RPC handlers once the view really has an id: a `CYGView` child
## allocates its own in `_ready`, which may run after this node's.
func _ensure_bound() -> bool:
	if _rpcs_bound:
		return true
	var target := view
	if target == null or not is_instance_valid(target) or target.view_id <= 0:
		return false
	bind_rpc(RPC_INPUT, _on_input_rpc)
	_rpcs_bound = true
	return true


func _authority_tick(tick: int) -> void:
	var move := input_vector
	var jump := jump_pressed
	jump_pressed = false

	_record_history(_ring_tick, _ring_input, tick, move, jump)
	_integrate(move, jump, tick)
	_ship_input(tick, move, jump)
	if authority_mode == AuthorityMode.OWNER_AUTHORITY:
		# Proxies have nothing to rewind: the property block is the whole truth.
		_publish()
	else:
		# The tick-stamped snapshot replaces the property stream here, so the same
		# bytes both correct a wrong prediction and prove the ledger is converging.
		_out_flags = FLAG_AUTHORITY_TICK | FLAG_VELOCITY | FLAG_GROUNDED
		_out_tick = tick
		_out_velocity = _velocity
		_out_grounded = _grounded
		_state_dirty = true


func _predict_tick(tick: int) -> void:
	_ensure_synchronizer()
	# A proxy predicts the authority's motion, not its own stick: the object's truth
	# is simulated elsewhere, and `cyg_input` says exactly what it integrated. Before
	# the first input arrives there is nothing to predict from, so the local intent
	# (a unit test driving this node directly) stands in.
	var move := input_vector
	var jump := jump_pressed
	if _authority_input_seen:
		move = _authority_move_for(tick)
		jump = _authority_jump_for(tick)
	jump_pressed = false
	_record_history(_ring_tick, _ring_input, tick, move, jump)

	var sync: CYGRollbackSynchronizer = _synchronizer
	if sync == null:
		_integrate(move, jump, tick)
		return
	# `predict()` runs the registered simulate for the input it records; if the ledger
	# never stepped (an unregistered id, say), step here so the local feel never stalls.
	var steps_before := _integrate_steps
	sync.predict(_object_id(), _pack_input(move, jump))
	if _integrate_steps == steps_before:
		_integrate(move, jump, tick)


## Publishes the non-pose state; `set_property` is authority-gated by the base.
func _publish() -> void:
	set_property(PROP_VELOCITY, _velocity)
	set_property(PROP_GROUNDED, _grounded)
	if sync_animation_speed:
		set_property(PROP_ANIMATION_SPEED, _animation_speed)


## ---------------------------------------------------------------- simulation

func _integrate(move: Vector2, jump: bool, tick: int) -> void:
	if _body == null or not is_instance_valid(_body):
		return
	var dt := _tick_delta()
	var target_x := clampf(move.x, -1.0, 1.0) * move_speed
	_velocity.x = move_toward(_velocity.x, target_x, acceleration * dt)

	if jump and _grounded:
		_velocity.y = -jump_impulse
	if not _grounded:
		# Gravity is stored negative as in 3D; Y grows downward here, so subtract it.
		_velocity.y -= gravity * dt

	# Godot's `move_and_slide` integrates over the physics step and takes no custom
	# delta, so the velocity is scaled onto the tick window for the call and the
	# resolved value is scaled back: a 30 Hz tick on a 60 Hz clock has to advance the
	# body two physics steps' worth of distance.
	var factor := _physics_factor(dt)
	_body.velocity = _velocity * factor
	_body.move_and_slide()
	_velocity = _body.velocity / factor
	# Grounding comes from the body's own contact resolution: it already ran the query.
	_grounded = _body.is_on_floor()
	if _grounded and _velocity.y > 0.0:
		_velocity.y = 0.0

	_body.velocity = _velocity
	_animation_speed = _planar_speed(_velocity)
	_integrate_steps += 1


func _tick_delta() -> float:
	var clock := network_time()
	var duration: float = clock.tick_duration_ms() / 1000.0
	if duration <= 0.000001 or duration > 0.25:
		duration = 1.0 / float(maxi(CYGNetwork.tick_rate, 1))
	return duration


## Velocity multiplier that turns one `move_and_slide` into one network tick.
func _physics_factor(dt: float) -> float:
	# A step of zero would divide the restored velocity by the factor and smear NaN
	# through it, so an unusable clock degrades to a 1:1 move.
	var step := get_physics_process_delta_time()
	if step <= 0.000001 or dt <= 0.000001:
		return 1.0
	return maxf(dt / step, 0.0001)


static func _planar_speed(value: Vector2) -> float:
	return absf(value.x)


## ----------------------------------------------------------- rollback plumbing

func _object_id() -> int:
	var target := view
	if target == null or not is_instance_valid(target):
		return 0
	return target.view_id


func _ensure_synchronizer() -> void:
	if _synchronizer != null:
		return
	var object_id := _object_id()
	if object_id <= 0:
		return
	_synchronizer = CYGRollbackSynchronizer.new(network_time())
	_synchronizer.register(object_id, _save_state, _restore_state, _simulate_tick)


## The snapshot the ledger rewinds to: velocity alone, grounding being a contact
## result the next simulate step re-derives from the body.
func _save_state() -> Variant:
	return _velocity


func _restore_state(state: Variant) -> void:
	if typeof(state) == TYPE_VECTOR2:
		_velocity = state
	elif typeof(state) == TYPE_DICTIONARY:
		var authority_state: Dictionary = state
		_velocity = authority_state.get("velocity", _velocity)
		_grounded = authority_state.get("grounded", _grounded)
	# A bare Vector2 carries no grounding, which is fine: the next simulate step
	# re-derives it from the body's contacts.
	_animation_speed = _planar_speed(_velocity)


## The ledger calls `simulate(input, value_tick)` with exactly the Variant handed to
## `predict()`, so the packed intent comes straight back rather than being looked up.
## An absent input (a null from an evicted ring slot) falls back to the tick history.
func _simulate_tick(input: Variant = null, value_tick: Variant = null) -> void:
	var tick := CYGNetwork.current_tick
	if typeof(value_tick) == TYPE_INT and int(value_tick) > 0:
		tick = int(value_tick)
	var move := _authority_move
	var jump := _authority_jump
	if typeof(input) == TYPE_INT:
		move = _unpack_move(int(input))
		jump = _unpack_jump(int(input))
	else:
		move = _history_move(tick)
		jump = _history_jump(tick)
	_integrate(move, jump, tick)


func _history_move(tick: int) -> Vector2:
	if tick <= 0:
		return input_vector
	var slot := tick & INPUT_MASK
	if _ring_tick[slot] == tick:
		return _unpack_move(_ring_input[slot])
	if _remote_tick[slot] == tick:
		return _unpack_move(_remote_input[slot])
	# WHY: a tick older than both rings must still replay with what the authority
	# actually did, not with whatever this peer is holding as its live intent.
	return _authority_move


func _history_jump(tick: int) -> bool:
	if tick <= 0:
		return false
	var slot := tick & INPUT_MASK
	if _ring_tick[slot] == tick:
		return _unpack_jump(_ring_input[slot])
	if _remote_tick[slot] == tick:
		return _unpack_jump(_remote_input[slot])
	return _authority_jump


## ------------------------------------------------------------- reconciliation

func _reconcile(authority_tick: int, authoritative_velocity: Vector2, authoritative_grounded: bool, has_grounded: bool) -> void:
	# The relay offers no ordering, and a snapshot of an older tick would rewind a
	# prediction that is already newer.
	if authority_tick <= _last_authority_tick:
		return
	_last_authority_tick = authority_tick
	_ensure_synchronizer()
	var previous := _velocity
	var next_grounded := _grounded
	if has_grounded:
		next_grounded = authoritative_grounded

	var sync: CYGRollbackSynchronizer = _synchronizer
	if sync == null:
		_velocity = authoritative_velocity
		_grounded = next_grounded
		_animation_speed = _planar_speed(_velocity)
		return

	var rewind_to := _local_authority_equivalent(authority_tick)
	var authority_state := {
		"velocity": authoritative_velocity,
		"grounded": next_grounded,
		"tick": authority_tick,
	}
	var mispredicted := sync.reconcile(_object_id(), rewind_to, authority_state)
	# The rewind left `_velocity` at what our own history re-simulated to; that is the
	# corrected value. With nothing rewound, the snapshot stands in for it.
	var corrected := _velocity if mispredicted > 0 else authoritative_velocity
	sync.acknowledge(_object_id(), rewind_to)
	_grounded = next_grounded
	_apply_correction(previous, corrected)


## Maps the authority's stamp onto the tick the ledger recorded when it predicted
## this state. Measured afresh whenever the implied rewind leaves the window, so a
## stall or a change of authority cannot make the ledger replay from nowhere.
func _local_authority_equivalent(authority_tick: int) -> int:
	var local_tick := network_time().tick()
	var implied := local_tick
	if _authority_anchored:
		implied = authority_tick - _authority_tick_offset
	if not _authority_anchored or implied > local_tick or local_tick - implied > INPUT_RING - 2:
		_authority_tick_offset = authority_tick - local_tick
		_authority_anchored = true
		return local_tick
	return implied


func _reset_authority_space() -> void:
	_authority_input_seen = false
	_authority_move = Vector2()
	_authority_jump = false
	_authority_anchored = false
	_authority_tick_offset = 0
	_last_authority_tick = -1


func _apply_correction(previous: Vector2, authoritative_velocity: Vector2) -> void:
	if authority_mode != AuthorityMode.REMOTE_AUTHORITY:
		_velocity = authoritative_velocity
		_animation_speed = _planar_speed(_velocity)
		return

	var difference := authoritative_velocity - previous
	if difference.length_squared() > max_correction_speed * max_correction_speed:
		_correcting = false
		_velocity = authoritative_velocity
		_animation_speed = _planar_speed(_velocity)
		return

	if difference.length_squared() <= CORRECTION_EPSILON * CORRECTION_EPSILON:
		_correcting = false
		_velocity = authoritative_velocity
		_animation_speed = _planar_speed(_velocity)
		return

	# Keep what was on screen and walk it onto the authoritative velocity; the pose
	# stream keeps moving meanwhile, so only the animation-facing value is nudged.
	_velocity = previous
	_correction_target = authoritative_velocity
	_correcting = true


func _process(delta: float) -> void:
	if not _correcting:
		return
	var alpha := clampf(delta / CORRECTION_BLEND_SECONDS, 0.0, 1.0)
	_velocity = _velocity.lerp(_correction_target, alpha)
	_animation_speed = _planar_speed(_velocity)
	if _velocity.distance_to(_correction_target) <= CORRECTION_EPSILON:
		_velocity = _correction_target
		_correcting = false


## ------------------------------------------------------------------ input wire

## ONE RPC per tick, authority to observers.
## Blob: [tick u32 BE][int8 x][int8 y][bool jump].
## WHY 8-bit intent: an input is a direction, not a distance — one byte per axis
## carries the -1..1 intent that twelve bytes of floats would, far below the
## resolution of a single tick of motion.
func _ship_input(tick: int, move: Vector2, jump: bool) -> void:
	if authority_mode != AuthorityMode.REMOTE_AUTHORITY or not CYGNetwork.is_connected_flag:
		return
	_blob_writer.reset()
	_blob_writer.write_uint32_be(tick)
	_blob_writer.write_byte(_axis_byte(move.x))
	_blob_writer.write_byte(_axis_byte(move.y))
	_blob_writer.write_bool(jump)
	_blob_args[0] = _blob_writer.to_bytes()
	send_rpc(RPC_INPUT, _blob_args, CYGOpcodes.Target.OBSERVERS)


func _on_input_rpc(args: Array) -> void:
	var blob := _arg_blob(args)
	if blob.size() < 7:
		return
	var reader := CYGBitReader.new()
	reader.attach(blob)
	var tick := int(reader.read_uint32_be())
	var move := Vector2(_read_axis(reader.read_byte()), _read_axis(reader.read_byte()))
	var jump := reader.read_bool()
	# Stamped in the authority's numbering, recorded under ours: the ring is read back
	# by tick during a prediction, and a foreign stamp could only ever miss.
	_record_history(_remote_tick, _remote_input, _local_authority_equivalent(tick), move, jump)
	_authority_move = move
	_authority_jump = jump
	_authority_input_seen = true


## The intent the authority integrated at `tick`: its exact bytes if they arrived,
## otherwise the newest one on hand, since a proxy that missed a packet should keep
## predicting the same motion rather than stall to zero.
func _authority_move_for(tick: int) -> Vector2:
	var slot := tick & INPUT_MASK
	if _remote_tick[slot] == tick:
		return _unpack_move(_remote_input[slot])
	return _authority_move


func _authority_jump_for(tick: int) -> bool:
	# Unlike the move axis, a held jump would re-launch the body, so a tick whose
	# exact input did not arrive gets no jump at all.
	var slot := tick & INPUT_MASK
	if _remote_tick[slot] == tick:
		return _unpack_jump(_remote_input[slot])
	return false


func _arg_blob(args: Array) -> PackedByteArray:
	if args.is_empty():
		return PackedByteArray()
	var first: Variant = args[0]
	if typeof(first) != TYPE_PACKED_BYTE_ARRAY:
		return PackedByteArray()
	return first


static func _pack_input(move: Vector2, jump: bool) -> int:
	return (_axis_byte(move.x) | (_axis_byte(move.y) << 8) | ((1 if jump else 0) << 16)) & 0xFFFFFF


static func _unpack_move(packed: int) -> Vector2:
	return Vector2(_read_axis(packed & 0xFF), _read_axis((packed >> 8) & 0xFF))


static func _unpack_jump(packed: int) -> bool:
	return ((packed >> 16) & 1) != 0


static func _axis_byte(value: float) -> int:
	if is_nan(value):
		return 0
	return clampi(int(roundf(value * 127.0)), -128, 127) & 0xFF


static func _read_axis(byte: int) -> float:
	var signed_byte := byte - 0x100 if byte >= 0x80 else byte
	return clampf(float(signed_byte) / 127.0, -1.0, 1.0)


## --------------------------------------------------------------------- history

func _record_history(ticks: Array[int], values: Array[int], tick: int, move: Vector2, jump: bool) -> void:
	var slot := tick & INPUT_MASK
	ticks[slot] = tick
	values[slot] = _pack_input(move, jump)


func _clear_history() -> void:
	for i in range(INPUT_RING):
		_ring_tick[i] = -1
		_ring_input[i] = 0
		_remote_tick[i] = -1
		_remote_input[i] = 0


func _on_remote_property(property_name: String, _old: Variant, value: Variant) -> void:
	# The integrating peer owns these values; an inbound copy is decoded only to keep
	# the shared property block aligned.
	if has_authority:
		return
	match property_name:
		PROP_VELOCITY:
			var next_velocity: Vector2 = value
			_velocity = next_velocity
			_animation_speed = _planar_speed(_velocity)
		PROP_GROUNDED:
			var next_grounded: bool = value
			_grounded = next_grounded
		PROP_ANIMATION_SPEED:
			var next_speed: float = value
			_animation_speed = next_speed
