class_name CYGPhysicsSync2D
extends CYGBehaviour
## Networked rigid-body state for a `RigidBody2D`: pose, linear and angular velocity
## and the engine's sleeping flag, plus impulses replayed everywhere.
##
## The authority samples the body once per network tick and ships only what moved
## past a threshold; proxies render the pose one interpolation window behind real time
## (cubic Hermite through the snapshot ring, tangents taken from the AUTHORITY's
## velocity rather than a differenced position) and, once the ring runs out,
## extrapolate with the last known velocity until the body is reported asleep. A crate
## at rest on the authority is at rest on every peer and generates no traffic: the
## sleep transition ships the resting pose once and sampling stops until something
## wakes it.
##
## Impulses travel as RPCs and body state as view deltas, because the Rust room relay
## forwards only CUSTOM_RPC (op 6, with target routing) and view deltas (op 5):
## PLAYER_INPUT (4) and REWIND_HIT (7) are dropped, so there is no dedicated physics
## channel to reserve. Peer-hosted physics prediction is deliberately absent: stock
## Godot cannot step the physics server, so a proxy that integrated for real would add
## its own gravity on top of the pose the authority already resolved. Godot's 2D space
## grows downward and project gravity is positive on Y, which makes that double pull a
## visible dive, not a rounding error. See `proxy_integration` for what a proxy does
## instead.
##
## Payload after the base sync-property block (this component registers no properties,
## so the block is empty on every peer and the registration order that defines it is
## identical everywhere):
##   [byte flags]                                     always, even when zero
##   FLAG_POSITION (1)      -> [vector2 position]     8 B, world-space floats
##   FLAG_ROTATION (2)      -> [single radians]       4 B
##   FLAG_LINEAR_VELOCITY (4) -> [vector2]            8 B, world units/s
##   FLAG_ANGULAR_VELOCITY (8) -> [single rad/s]      4 B, clockwise positive
##   FLAG_SLEEP_STATE (16)  -> [bool sleeping]        1 B
##   FLAG_TELEPORT (32)     -> no payload
## Fields appear in ascending flag-bit order and only the flag decides whether a field
## is present, so a reader on a peer that ignores the values consumes exactly the bytes
## the writer wrote. There is no packed vector2 codec, so velocity rides as full
## floats; half precision would quantise a 60 units/s shove to about 0.06.
##
## Impulse blob, shared by `cyg_phys_impulse` (authority -> others) and
## `cyg_phys_impulse_req` (proxy -> the peer holding authority; it rides a broadcast
## because the relay has no owner-unicast path for scene objects and only the authority
## acts on it):
##   [byte kind] 0 = impulse at point, 1 = central impulse, 2 = torque impulse
##   kind 0 -> [vector2 impulse][vector2 world position]  17 B
##   kind 1 -> [vector2 impulse]                          9 B
##   kind 2 -> [single torque]                            5 B
## A 2D torque is a scalar, so it is carried in the vector's x lane in memory and
## written as one float on the wire.
##
## Design derives from Netfox's MIT-licensed physics extras (attribution in
## LICENSE-NETFOX.md). Independent GDScript; no Netfox source text is copied.

enum ProxyIntegration {
	## The body is frozen in kinematic mode and driven by the interpolated pose. It
	## still collides and pushes dynamic bodies it meets, but resolves no forces of
	## its own: the authority's stream is the only integrator in the room.
	FREEZE_KINEMATIC,
	## Frozen and static: the cheapest puppet, for props nothing may push around.
	FREEZE_STATIC,
	## The proxy keeps simulating and chases the authority's VELOCITY, correcting
	## position drift. Use it where local contacts with this peer's own players matter
	## more than a pixel-exact pose, such as a crate someone is pushing.
	DYNAMIC_VELOCITY_DRIVE,
}

const BUFFER_CAPACITY := 32
const BUFFER_MASK := BUFFER_CAPACITY - 1
const MAX_EXTRAPOLATION := 0.15
## A kick is worth predicting longer than plain motion: the peer that got kicked is
## the one holding the newest information about where the crate is heading.
const NUDGE_EXTRA_EXTRAPOLATION := 0.35
const NUDGE_TIME_CONSTANT := 0.25
const NUDGE_EPSILON_SQR := 1e-8

const POSITION_THRESHOLD_SQR := 0.01 * 0.01
const ROTATION_THRESHOLD_DEG := 0.25
const LINEAR_VELOCITY_THRESHOLD_SQR := 0.02 * 0.02
const ANGULAR_VELOCITY_THRESHOLD := 0.05
## WHY: a >5 unit jump is a respawn or a launcher, not motion; interpolating that gap
## looks like the crate flew across the screen, so both sides snap and clear instead.
const TELEPORT_DISTANCE_SQR := 5.0 * 5.0
## Dynamic proxies only: drift past this is repositioned outright, smaller drift is
## blended, so a body that diverged does not rubber-band.
const DRIFT_SNAP_DISTANCE_SQR := 1.0 * 1.0
const DRIFT_BLEND_DISTANCE_SQR := 0.001 * 0.001
const DRIFT_BLEND_RATE := 8.0

const FLAG_POSITION := 1
const FLAG_ROTATION := 2
const FLAG_LINEAR_VELOCITY := 4
const FLAG_ANGULAR_VELOCITY := 8
const FLAG_SLEEP_STATE := 16
const FLAG_TELEPORT := 32

const RPC_IMPULSE := "cyg_phys_impulse"
const RPC_IMPULSE_REQUEST := "cyg_phys_impulse_req"

const KIND_IMPULSE := 0
const KIND_CENTRAL_IMPULSE := 1
const KIND_TORQUE_IMPULSE := 2

## How a peer without authority keeps the body in step with the authority.
@export var proxy_integration: ProxyIntegration = ProxyIntegration.FREEZE_KINEMATIC
## Interpolation window in seconds: the pose a proxy renders is this far behind now.
@export var interpolation_delay: float = 0.1
## Ship linear velocity, so proxies can extrapolate and chase a moving crate.
@export var sync_linear_velocity: bool = true
## Ship angular velocity, radians per second, clockwise positive.
@export var sync_angular_velocity: bool = true
## Ship sleeping transitions; off means a proxy extrapolates a resting crate forever.
@export var sync_sleep_state: bool = true

var _body: RigidBody2D = null

# Pre-sized parallel ring arrays: no allocation on the hot path, no inner-class
# array typing to trip the compiler over.
var _time_ring: Array[float] = []
var _pos_ring: Array[Vector2] = []
var _rot_ring: Array[float] = []
var _lin_ring: Array[Vector2] = []
var _ang_ring: Array[float] = []
var _head := 0
var _count := 0

var _physics_dirty := false
var _out_flags := 0
var _out_position := Vector2()
var _out_rotation := 0.0
var _out_linear_velocity := Vector2()
var _out_angular_velocity := 0.0
var _out_sleeping := false

var _last_sent_position := Vector2()
var _last_sent_rotation := 0.0
var _last_sent_linear_velocity := Vector2()
var _last_sent_angular_velocity := 0.0
var _has_sent_baseline := false
var _sent_sleeping := false
var _force_velocity_delta := false

var _last_incoming_position := Vector2()
var _last_incoming_rotation := 0.0
var _last_incoming_linear_velocity := Vector2()
var _last_incoming_angular_velocity := 0.0

var _network_linear_velocity := Vector2()
var _network_angular_velocity := 0.0
var _network_sleeping := false

# Estimate scratch, refilled each frame from the ring without allocating.
var _est_position := Vector2()
var _est_rotation := 0.0
var _est_linear_velocity := Vector2()
var _est_angular_velocity := 0.0

# A locally predicted kick: velocity folded into the extrapolation plus an offset that
# decays back to zero, so the authority's stream resettles the pose with no seam.
var _nudge_linear := Vector2()
var _nudge_angular := 0.0
var _nudge_offset := Vector2()
var _nudge_rotation := 0.0

var _role_applied := false
var _role_is_authority := false
var _ticked := false
var _rpcs_bound := false
var _blob_writer: CYGBitPacker = null
var _blob_args: Array = [null]


func _ready() -> void:
	_body = get_parent() as RigidBody2D
	if _body == null:
		push_warning("CYGPhysicsSync2D on %s: parent is not a RigidBody2D; nothing to sync." % name)
		return
	for _i in range(BUFFER_CAPACITY):
		_time_ring.append(0.0)
		_pos_ring.append(Vector2())
		_rot_ring.append(0.0)
		_lin_ring.append(Vector2())
		_ang_ring.append(0.0)
	_blob_writer = CYGBitPacker.new()
	register_as_observable()
	CYGNetwork.network_tick.connect(_on_network_tick)
	_ticked = true


func _exit_tree() -> void:
	if _ticked:
		CYGNetwork.network_tick.disconnect(_on_network_tick)
		_ticked = false
	super._exit_tree()


func cyg_on_spawned() -> void:
	_reset_state()


func cyg_on_despawned() -> void:
	_reset_state()


## The peer doing the integrating just changed, so every snapshot and every cached role
## decision here describes a body this peer is no longer allowed to own.
func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	_reset_state()


## Newest velocity the room reports for this body: on the authority the body's own
## velocity at the last tick, on a proxy the value the wire carried.
var network_linear_velocity: Vector2:
	get:
		return _network_linear_velocity


## Radians per second, clockwise positive.
var network_angular_velocity: float:
	get:
		return _network_angular_velocity


## True while the authority's body is asleep, meaning the crate is at rest everywhere.
var is_network_sleeping: bool:
	get:
		return _network_sleeping


func has_delta() -> bool:
	return _physics_dirty or super.has_delta()


func serialize(writer: CYGBitPacker) -> void:
	super.serialize(writer)
	# The flags byte is written even when zero so deserialize's fixed layout stays
	# symmetric with sync-property-only deltas.
	writer.write_byte(_out_flags)
	if not _physics_dirty:
		return

	if (_out_flags & FLAG_POSITION) != 0:
		writer.write_vector2(_out_position)
	if (_out_flags & FLAG_ROTATION) != 0:
		writer.write_single(_out_rotation)
	if (_out_flags & FLAG_LINEAR_VELOCITY) != 0:
		writer.write_vector2(_out_linear_velocity)
	if (_out_flags & FLAG_ANGULAR_VELOCITY) != 0:
		writer.write_single(_out_angular_velocity)
	if (_out_flags & FLAG_SLEEP_STATE) != 0:
		writer.write_bool(_out_sleeping)
	_out_flags = 0
	_physics_dirty = false


func deserialize(reader: CYGBitReader) -> void:
	super.deserialize(reader)

	var flags := reader.read_byte()
	# Fields the delta does not carry keep the newest value known, so a velocity-only
	# delta still describes the whole body.
	var position := _last_incoming_position
	var rotation := _last_incoming_rotation
	var linear_velocity := _last_incoming_linear_velocity
	var angular_velocity := _last_incoming_angular_velocity
	var sleeping := _network_sleeping

	if (flags & FLAG_POSITION) != 0:
		position = reader.read_vector2()
	if (flags & FLAG_ROTATION) != 0:
		rotation = reader.read_single()
	if (flags & FLAG_LINEAR_VELOCITY) != 0:
		linear_velocity = reader.read_vector2()
	if (flags & FLAG_ANGULAR_VELOCITY) != 0:
		angular_velocity = reader.read_single()
	if (flags & FLAG_SLEEP_STATE) != 0:
		sleeping = reader.read_bool()

	# The authority decoded the block only to keep the shared reader aligned; its own
	# simulation owns the body.
	if has_authority:
		return

	_last_incoming_position = position
	_last_incoming_rotation = rotation
	_last_incoming_linear_velocity = linear_velocity
	_last_incoming_angular_velocity = angular_velocity
	_network_linear_velocity = linear_velocity
	_network_angular_velocity = angular_velocity
	_network_sleeping = sleeping

	var now: float = Time.get_ticks_usec() / 1000000.0
	var teleport := (flags & FLAG_TELEPORT) != 0
	if not teleport and _count > 0:
		teleport = (position - _pos_ring[_ring_index(_count - 1)]).length_squared() >= TELEPORT_DISTANCE_SQR
	if teleport or _count == 0:
		# A teleport is not motion: drop the ring so the respawn cannot smear, pin the
		# pose, and hand the extrapolation no velocity to run away with.
		_clear_ring()
		_reset_nudge()
		_push_snapshot(position, rotation, Vector2(), 0.0, now)
		_write_body_pose(position, rotation, linear_velocity, angular_velocity, sleeping)
		return

	_push_snapshot(position, rotation, linear_velocity, angular_velocity, now)


## -------------------------------------------------------------------- public API

## Kicks the body at a world-space point and tells every peer, so the crate travels the
## same distance on the peer that watched it as on the one that simulated it.
func apply_network_impulse(impulse: Vector2, position: Vector2) -> void:
	_dispatch_impulse(KIND_IMPULSE, impulse, position)


## Kicks the body through its center of mass: translation with no spin.
func apply_network_central_impulse(impulse: Vector2) -> void:
	_dispatch_impulse(KIND_CENTRAL_IMPULSE, impulse, Vector2())


## Spins the body with a torque impulse (rad/s kick). A held twist is
## `RigidBody2D.add_constant_torque`, which is local simulation, not network state.
func add_network_torque(torque_impulse: float) -> void:
	_dispatch_impulse(KIND_TORQUE_IMPULSE, Vector2(torque_impulse, 0.0), Vector2())


## Sets the body velocity and ships it, so a launcher or a knockback sends the crate off
## on every peer rather than only on the one that called it.
func set_network_velocity(linear: Vector2, angular: float = 0.0) -> void:
	if _body == null:
		return
	_network_linear_velocity = linear
	_network_angular_velocity = angular
	if has_authority:
		# Velocity first, sleep state last: writing a velocity wakes a sleeping body, so
		# the resting flag has to be the final write to be the state the engine keeps.
		_body.linear_velocity = linear
		_body.angular_velocity = angular
		_body.set_sleeping(false)
		_network_sleeping = false
		_out_position = _body.global_position
		_out_rotation = _body.global_transform.get_rotation()
		_out_linear_velocity = linear
		_out_angular_velocity = angular
		_out_sleeping = false
		_out_flags |= FLAG_LINEAR_VELOCITY | FLAG_ANGULAR_VELOCITY | FLAG_SLEEP_STATE
		_physics_dirty = true
	elif proxy_integration == ProxyIntegration.DYNAMIC_VELOCITY_DRIVE:
		_body.linear_velocity = linear
		_body.angular_velocity = angular
	else:
		_set_newest_velocity(linear, angular)


## Teleports the body and broadcasts the teleport flag, so a respawn or a transfer does
## not smear across the screen through a ring full of the old position.
func snap_transform(position: Vector2, rotation: float = 0.0) -> void:
	if _body == null:
		return
	var sleeping := _body.is_sleeping()
	var linear_velocity := Vector2() if sleeping else _body.linear_velocity
	var angular_velocity := 0.0 if sleeping else _body.angular_velocity
	_write_body_pose(position, rotation, linear_velocity, angular_velocity, sleeping)
	_last_sent_position = position
	_last_sent_rotation = rotation
	_last_incoming_position = position
	_last_incoming_rotation = rotation

	if has_authority:
		_out_position = position
		_out_rotation = rotation
		_out_linear_velocity = linear_velocity
		_out_angular_velocity = angular_velocity
		_out_sleeping = sleeping
		_out_flags |= FLAG_POSITION | FLAG_ROTATION | FLAG_LINEAR_VELOCITY \
			| FLAG_ANGULAR_VELOCITY | FLAG_SLEEP_STATE | FLAG_TELEPORT
		_physics_dirty = true
	else:
		_clear_ring()
		_reset_nudge()
		var now: float = Time.get_ticks_usec() / 1000000.0
		_push_snapshot(position, rotation, Vector2(), 0.0, now)


## Puts the body to rest now. Proxies stop extrapolating as soon as the transition
## reaches them, so with `sync_sleep_state` off this only affects this peer.
func sleep_now() -> void:
	if _body == null:
		return
	if has_authority:
		_body.set_sleeping(true)
	_network_sleeping = true
	_nudge_linear = Vector2()
	_nudge_angular = 0.0


## Wakes the body so forces, impulses and proxy extrapolation start moving again.
func wake_now() -> void:
	if _body == null:
		return
	if has_authority:
		_body.set_sleeping(false)
	_network_sleeping = false


## --------------------------------------------------------------------- tick loop

func _on_network_tick(_tick: int) -> void:
	if _body == null or not is_instance_valid(_body):
		return
	_apply_role()
	if not _ensure_bound():
		return
	if has_authority:
		_authority_tick()


## Binds the RPC handlers once the view really has an id: a `CYGView` child allocates its
## own in `_ready`, which may run after this node's.
func _ensure_bound() -> bool:
	if _rpcs_bound:
		return true
	var target := view
	if target == null or not is_instance_valid(target) or target.view_id <= 0:
		return false
	bind_rpc(RPC_IMPULSE, _on_impulse_rpc)
	bind_rpc(RPC_IMPULSE_REQUEST, _on_impulse_request_rpc)
	_rpcs_bound = true
	return true


## Adopts the integration mode the current ownership demands. Running it on a change
## rather than every tick keeps `freeze` writes out of the hot path.
func _apply_role() -> void:
	var authority := has_authority
	if _role_applied and authority == _role_is_authority:
		return
	_role_applied = true
	_role_is_authority = authority
	_clear_ring()
	_reset_nudge()
	_has_sent_baseline = false
	_physics_dirty = false
	_force_velocity_delta = false
	_out_flags = 0
	_last_sent_position = _body.global_position
	_last_sent_rotation = _body.global_transform.get_rotation()
	_last_sent_linear_velocity = Vector2()
	_last_sent_angular_velocity = 0.0

	if authority:
		_body.freeze = false
		_sent_sleeping = _body.is_sleeping()
	else:
		# WHY freeze and not a live simulation: a proxy that integrated would add its own
		# gravity and contacts on top of the authority's resolved pose, so the stream and
		# the local sim fight and the crate shivers or dives. A frozen body takes the
		# written transform as truth, still collides, and cannot wake itself. Kinematic mode
		# is the variant that also shoves dynamic bodies out of the way, which a sliding
		# crate should do.
		if proxy_integration == ProxyIntegration.DYNAMIC_VELOCITY_DRIVE:
			_body.freeze = false
		else:
			_body.freeze = true
			_body.freeze_mode = RigidBody2D.FREEZE_MODE_KINEMATIC \
				if proxy_integration == ProxyIntegration.FREEZE_KINEMATIC \
				else RigidBody2D.FREEZE_MODE_STATIC
		_sent_sleeping = false
		# A frozen body reports itself asleep, and that is the honest default until the
		# authority says otherwise: rest, then follow.
		_network_sleeping = _body.is_sleeping()
		_network_linear_velocity = Vector2()
		_network_angular_velocity = 0.0
		_last_incoming_position = _body.global_position
		_last_incoming_rotation = _body.global_transform.get_rotation()
		_last_incoming_linear_velocity = Vector2()
		_last_incoming_angular_velocity = 0.0


func _authority_tick() -> void:
	var sleeping := _body.is_sleeping()
	if sleeping and _sent_sleeping:
		# Asleep here and already reported asleep everywhere: the crate is at rest on every
		# peer, so nothing is sampled, nothing is dirty, nothing goes on the wire.
		return

	var forced := _force_velocity_delta
	_force_velocity_delta = false
	var position := _body.global_position
	var rotation := _body.global_transform.get_rotation()
	var linear_velocity := _body.linear_velocity
	var angular_velocity := _body.angular_velocity
	_network_linear_velocity = linear_velocity
	_network_angular_velocity = angular_velocity
	_network_sleeping = sleeping

	var flags := 0
	var position_delta_sqr := (position - _last_sent_position).length_squared()
	if not _has_sent_baseline:
		# The first sample after a spawn or an ownership change is both the threshold
		# baseline and a teleport: proxies just dropped their rings, and their copy of the
		# pose is whatever they froze it at.
		_has_sent_baseline = true
		flags = FLAG_POSITION | FLAG_ROTATION | FLAG_TELEPORT
		if sync_linear_velocity:
			flags |= FLAG_LINEAR_VELOCITY
		if sync_angular_velocity:
			flags |= FLAG_ANGULAR_VELOCITY
		if sync_sleep_state:
			flags |= FLAG_SLEEP_STATE
	else:
		if position_delta_sqr >= POSITION_THRESHOLD_SQR:
			flags |= FLAG_POSITION
		if rad_to_deg(absf(wrapf(rotation - _last_sent_rotation, -PI, PI))) > ROTATION_THRESHOLD_DEG:
			flags |= FLAG_ROTATION
		if sync_linear_velocity and (forced \
				or (linear_velocity - _last_sent_linear_velocity).length_squared() >= LINEAR_VELOCITY_THRESHOLD_SQR):
			flags |= FLAG_LINEAR_VELOCITY
		if sync_angular_velocity and (forced or absf(angular_velocity - _last_sent_angular_velocity) >= ANGULAR_VELOCITY_THRESHOLD):
			flags |= FLAG_ANGULAR_VELOCITY
		if sync_sleep_state and sleeping != _sent_sleeping:
			# Ship the exact resting pose with the transition: a proxy that stops
			# extrapolating has to be left sitting where the authority left the crate.
			flags |= FLAG_SLEEP_STATE | FLAG_POSITION | FLAG_ROTATION
		if position_delta_sqr >= TELEPORT_DISTANCE_SQR:
			flags |= FLAG_TELEPORT

	if flags == 0:
		return

	_out_flags |= flags
	_out_position = position
	_out_rotation = rotation
	_out_linear_velocity = linear_velocity
	_out_angular_velocity = angular_velocity
	_out_sleeping = sleeping
	_last_sent_position = position
	_last_sent_rotation = rotation
	_last_sent_linear_velocity = linear_velocity
	_last_sent_angular_velocity = angular_velocity
	_sent_sleeping = sleeping
	_physics_dirty = true


## --------------------------------------------------------------- snapshot ring

func _push_snapshot(position: Vector2, rotation: float, linear_velocity: Vector2, angular_velocity: float, now: float) -> void:
	_time_ring[_head] = now
	_pos_ring[_head] = position
	_rot_ring[_head] = rotation
	_lin_ring[_head] = linear_velocity
	_ang_ring[_head] = angular_velocity
	_head = (_head + 1) & BUFFER_MASK
	if _count < BUFFER_CAPACITY:
		_count += 1


func _ring_index(index_from_oldest: int) -> int:
	return (_head - _count + index_from_oldest) & BUFFER_MASK


func _clear_ring() -> void:
	_head = 0
	_count = 0


func _set_newest_velocity(linear: Vector2, angular: float) -> void:
	if _count == 0:
		return
	var newest := _ring_index(_count - 1)
	_lin_ring[newest] = linear
	_ang_ring[newest] = angular


## --------------------------------------------------------------- local kick blend

## Integrates the predicted kick into the pose offset and decays everything back to
## zero, so the authority's own samples take the crate over without a visible seam.
func _advance_nudge(delta: float) -> void:
	if delta <= 0.0 or not _nudge_active():
		return
	_nudge_offset += _nudge_linear * delta
	if absf(_nudge_angular) > 0.0001:
		_nudge_rotation = wrapf(_nudge_rotation + _nudge_angular * delta, -PI, PI)
	var decay: float = exp(-delta / NUDGE_TIME_CONSTANT)
	_nudge_linear *= decay
	_nudge_angular *= decay
	_nudge_offset *= decay
	_nudge_rotation *= decay


func _nudge_active() -> bool:
	var active: bool = _nudge_linear.length_squared() > NUDGE_EPSILON_SQR \
		or absf(_nudge_angular) > 0.0001
	return active


func _reset_nudge() -> void:
	_nudge_linear = Vector2()
	_nudge_angular = 0.0
	_nudge_offset = Vector2()
	_nudge_rotation = 0.0


## ------------------------------------------------------------------- rendering

## Frozen proxies: place the puppet exactly where the lagged estimate says it is.
func _process(delta: float) -> void:
	if _body == null or _count == 0 or has_authority:
		return
	if proxy_integration == ProxyIntegration.DYNAMIC_VELOCITY_DRIVE:
		return
	_advance_nudge(delta)
	var now: float = Time.get_ticks_usec() / 1000000.0
	_compute_estimate(now - maxf(interpolation_delay, 0.0))
	_body.global_transform = Transform2D(wrapf(_est_rotation + _nudge_rotation, -PI, PI), _est_position + _nudge_offset)


## Dynamic proxies chase velocity rather than pose, so local contacts still resolve;
## that has to happen in the physics step, where the engine will use the values.
func _physics_process(delta: float) -> void:
	if _body == null or _count == 0 or has_authority:
		return
	if proxy_integration != ProxyIntegration.DYNAMIC_VELOCITY_DRIVE:
		return
	_advance_nudge(delta)
	var now: float = Time.get_ticks_usec() / 1000000.0
	_compute_estimate(now - maxf(interpolation_delay, 0.0))
	_body.linear_velocity = _est_linear_velocity + _nudge_linear
	_body.angular_velocity = _est_angular_velocity + _nudge_angular

	var drift := _est_position - _body.global_position
	var drift_sqr := drift.length_squared()
	if drift_sqr >= DRIFT_SNAP_DISTANCE_SQR:
		_body.global_transform = Transform2D(_est_rotation, _est_position)
	elif drift_sqr > DRIFT_BLEND_DISTANCE_SQR:
		_body.global_position += drift * minf(1.0, delta * DRIFT_BLEND_RATE)


## Fills the estimate scratch from the ring at `render_time`. Allocates nothing.
func _compute_estimate(render_time: float) -> void:
	var newest := _ring_index(_count - 1)
	var oldest := _ring_index(0)
	var asleep := _network_sleeping

	if render_time >= _time_ring[newest]:
		var extrap: float = render_time - _time_ring[newest]
		var limit: float = MAX_EXTRAPOLATION
		if _nudge_active():
			limit += NUDGE_EXTRA_EXTRAPOLATION
		if extrap > limit:
			extrap = limit
		# A crate the room says is asleep must not slide on a proxy either.
		var linear := Vector2() if asleep else _lin_ring[newest] + _nudge_linear
		var angular := 0.0 if asleep else _ang_ring[newest] + _nudge_angular
		_est_position = _pos_ring[newest] + linear * extrap
		_est_rotation = wrapf(_rot_ring[newest] + angular * extrap, -PI, PI)
		_est_linear_velocity = linear
		_est_angular_velocity = angular
		return

	if render_time <= _time_ring[oldest]:
		_est_position = _pos_ring[oldest]
		_est_rotation = _rot_ring[oldest]
		_est_linear_velocity = _lin_ring[oldest]
		_est_angular_velocity = _ang_ring[oldest]
		_gate_estimate_velocity(asleep)
		return

	for i in range(_count - 1):
		var a := _ring_index(i)
		var b := _ring_index(i + 1)
		if render_time < _time_ring[a] or render_time > _time_ring[b]:
			continue

		var span: float = _time_ring[b] - _time_ring[a]
		if span <= 0.000001:
			_est_position = _pos_ring[b]
			_est_rotation = _rot_ring[b]
			_est_linear_velocity = _lin_ring[b]
			_est_angular_velocity = _ang_ring[b]
			_gate_estimate_velocity(asleep)
			return

		var t: float = (render_time - _time_ring[a]) / span
		var t2: float = t * t
		var t3: float = t2 * t
		# WHY: cubic Hermite through both snapshots with the authority's velocities as the
		# tangents keeps C1 continuity across joins and follows a real arc under gravity,
		# which a position differenced between packets could not.
		_est_position = (2.0 * t3 - 3.0 * t2 + 1.0) * _pos_ring[a] \
			+ (t3 - 2.0 * t2 + t) * _lin_ring[a] * span \
			+ (-2.0 * t3 + 3.0 * t2) * _pos_ring[b] \
			+ (t3 - t2) * _lin_ring[b] * span
		_est_rotation = lerp_angle(_rot_ring[a], _rot_ring[b], t)
		_est_linear_velocity = _lin_ring[a].lerp(_lin_ring[b], t)
		_est_angular_velocity = lerp(_ang_ring[a], _ang_ring[b], t)
		_gate_estimate_velocity(asleep)
		return

	_est_position = _pos_ring[newest]
	_est_rotation = _rot_ring[newest]
	_est_linear_velocity = _lin_ring[newest]
	_est_angular_velocity = _ang_ring[newest]
	_gate_estimate_velocity(asleep)


func _gate_estimate_velocity(asleep: bool) -> void:
	if asleep:
		_est_linear_velocity = Vector2()
		_est_angular_velocity = 0.0


## Writes a pose the whole room agreed on. Order matters: transform, then velocity, then
## sleep state, because the first two wake a sleeping body by themselves.
func _write_body_pose(position: Vector2, rotation: float, linear_velocity: Vector2, angular_velocity: float, sleeping: bool) -> void:
	if _body == null:
		return
	_body.global_transform = Transform2D(rotation, position)
	if proxy_integration != ProxyIntegration.DYNAMIC_VELOCITY_DRIVE:
		# A frozen body keeps no velocity of its own; the ring is the extrapolation source.
		return
	_body.linear_velocity = Vector2() if sleeping else linear_velocity
	_body.angular_velocity = 0.0 if sleeping else angular_velocity
	_body.set_sleeping(sleeping)


## ------------------------------------------------------------- impulse plumbing

func _dispatch_impulse(kind: int, primary: Vector2, secondary: Vector2) -> void:
	if _body == null:
		return
	if has_authority:
		_execute_impulse(kind, primary, secondary)
		_ship_impulse(RPC_IMPULSE, kind, primary, secondary)
	else:
		# A proxy may well be the peer that kicked the crate. Ask the authority to do the
		# real integration, and start the local copy moving now so the kick does not arrive
		# a full round trip late.
		_predict_impulse(kind, primary)
		_ship_impulse(RPC_IMPULSE_REQUEST, kind, primary, secondary)


func _execute_impulse(kind: int, primary: Vector2, secondary: Vector2) -> void:
	match kind:
		KIND_IMPULSE:
			_body.apply_impulse(primary, secondary)
		KIND_CENTRAL_IMPULSE:
			_body.apply_central_impulse(primary)
		KIND_TORQUE_IMPULSE:
			_body.apply_torque_impulse(primary.x)
		_:
			return
	_body.set_sleeping(false)
	_network_sleeping = false
	# The engine folds an impulse in during the next integration step, so this peer does
	# not know the resulting velocity yet: the next sample has to ship it unconditionally.
	_force_velocity_delta = true


## Answers a proxy's request: integrate for real and rebroadcast, so every peer including
## the requester sees one and the same kick.
func _on_impulse_request_rpc(args: Array) -> void:
	if _body == null or not has_authority:
		return
	var parsed := _read_impulse_blob(args)
	if parsed.is_empty():
		return
	var kind := int(parsed[0])
	var primary: Vector2 = parsed[1]
	var secondary: Vector2 = parsed[2]
	_execute_impulse(kind, primary, secondary)
	_ship_impulse(RPC_IMPULSE, kind, primary, secondary)


func _on_impulse_rpc(args: Array) -> void:
	if _body == null or has_authority:
		# The sender already integrated the kick, and an authority holding a better local
		# value (a Unity peer's, say) is not going to be told what its own body did.
		return
	var parsed := _read_impulse_blob(args)
	if parsed.is_empty():
		return
	var kind := int(parsed[0])
	var primary: Vector2 = parsed[1]
	var secondary: Vector2 = parsed[2]
	if proxy_integration == ProxyIntegration.DYNAMIC_VELOCITY_DRIVE:
		_execute_impulse(kind, primary, secondary)
	else:
		_predict_impulse(kind, primary)


## Predicts a kick on a frozen body, which cannot take an impulse: the velocity goes into
## the extrapolation instead, so the crate reacts at once and the decaying offset lets the
## authority's stream resettle it without a visible seam.
func _predict_impulse(kind: int, primary: Vector2) -> void:
	var inverse_mass: float = 1.0 / maxf(_body.mass, 0.001)
	match kind:
		KIND_IMPULSE, KIND_CENTRAL_IMPULSE:
			_nudge_linear += primary * inverse_mass
		KIND_TORQUE_IMPULSE:
			_nudge_angular += primary.x * inverse_mass
		_:
			return
	_network_sleeping = false


func _ship_impulse(rpc_name: String, kind: int, primary: Vector2, secondary: Vector2) -> void:
	if not CYGNetwork.is_connected_flag:
		return
	_blob_writer.reset()
	_blob_writer.write_byte(kind)
	if kind == KIND_TORQUE_IMPULSE:
		# A 2D torque is a scalar; it needs one float, not a vector.
		_blob_writer.write_single(primary.x)
	else:
		_blob_writer.write_vector2(primary)
		if kind == KIND_IMPULSE:
			_blob_writer.write_vector2(secondary)
	_blob_args[0] = _blob_writer.to_bytes()
	# OTHERS, because the sender has already acted on the kick: the router echoes ALL
	# locally, which would apply the impulse twice on the authority.
	send_rpc(rpc_name, _blob_args, CYGOpcodes.Target.OTHERS)


## Reads the kind-tagged blob into `[kind, primary, secondary]`, or an empty Array when it
## is too short to trust. A torque arrives in `primary.x`.
func _read_impulse_blob(args: Array) -> Array:
	if args.is_empty():
		return []
	var first: Variant = args[0]
	if typeof(first) != TYPE_PACKED_BYTE_ARRAY:
		return []
	var blob: PackedByteArray = first
	if blob.size() < 5:
		return []
	var reader := CYGBitReader.new()
	reader.attach(blob)
	var kind := reader.read_byte()
	var primary := Vector2()
	var secondary := Vector2()
	match kind:
		KIND_TORQUE_IMPULSE:
			primary = Vector2(reader.read_single(), 0.0)
		KIND_CENTRAL_IMPULSE:
			if blob.size() < 9:
				return []
			primary = reader.read_vector2()
		KIND_IMPULSE:
			if blob.size() < 17:
				return []
			primary = reader.read_vector2()
			secondary = reader.read_vector2()
		_:
			return []
	return [kind, primary, secondary]


## ------------------------------------------------------------------ state reset

func _reset_state() -> void:
	_physics_dirty = false
	_out_flags = 0
	_out_position = Vector2()
	_out_rotation = 0.0
	_out_linear_velocity = Vector2()
	_out_angular_velocity = 0.0
	_out_sleeping = false
	_has_sent_baseline = false
	_sent_sleeping = false
	_force_velocity_delta = false
	_last_sent_position = Vector2()
	_last_sent_rotation = 0.0
	_last_sent_linear_velocity = Vector2()
	_last_sent_angular_velocity = 0.0
	_network_linear_velocity = Vector2()
	_network_angular_velocity = 0.0
	_network_sleeping = false
	_role_applied = false
	_role_is_authority = false
	_reset_nudge()
	_clear_ring()
	if _body != null and is_instance_valid(_body):
		_last_incoming_position = _body.global_position
		_last_incoming_rotation = _body.global_transform.get_rotation()
		_last_incoming_linear_velocity = Vector2()
		_last_incoming_angular_velocity = 0.0
