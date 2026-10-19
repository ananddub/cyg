class_name CYGLagComp3D
extends CYGBehaviour
## 3D lag compensation: rewind raycasts against tick-stored collider history,
## then adjudicate the hit peer-to-peer.
##
## WHAT this replaces, and why the flow differs from the Unity and Unreal SDKs:
## those send a `RewindHit` claim (opcode 7) to a server that rebuilds the
## collider poses, re-tests the ray and applies the hit. The Rust room relay
## declares op 7 but never forwards it — like op 4 (PlayerInput) it is dropped —
## so no server anywhere in this stack will ever roll back a shot. Everything a
## peer knows about another body arrives as view deltas or RPCs. This component
## therefore rewinds what the shooter already has: the local *replica* history
## kept by each `CYGLagHitbox3D`, whose tick keys come from the same
## `CYGNetworkTime` that stamps state deltas, so a rewind here lands on the same
## tick the room renders. "Was the hit accepted" is decided twice, each time by
## a local deterministic rewind, never by a stranger: once by the shooter
## (`validate()` before `report_hit()` emits the claim RPC), and once by the
## peer that owns the target view, whose `CYGLagHitbox3D` components recorded
## that body's true poses. A shooter can aim at a stale replica and get clamped
## history — the victim-side re-check is the veto. `report_hit()` rides
## `CYGView.send_rpc` with `CYGOpcodes.Target.ALL`; there is no other path to
## the victim in this relay.
##
## Stale replicas: a proxy's hitbox records only the poses its own view deltas
## produced. A body that stopped updating clamps every older query to its newest
## recorded tick (the pose everyone still sees), and a body that has never
## received its first delta has no history at all, so queries skip it rather
## than hit a phantom at the spawn origin.
##
## Rewind never mutates the live scene: candidates are solved analytically
## against the recorded poses (sphere and world-AABB tests, the same primitives
## as `CYGRewindMath` in the Unreal SDK), and the live `PhysicsDirectSpaceState3D`
## — taken from the viewport's `World3D`, it cannot be instantiated — is only
## consulted when `occlusion_mask` is non-zero, to ask the present world whether
## a static wall blocks the rewound hit. The live collider's own body is
## excluded from that probe. With `occlusion_mask == 0` a query allocates
## nothing: the hit result, the maths scratch and every sampled pose are reused
## members.
##
## Tolerance windows, matching the references' 0.3 s rewind limit: a claim whose
## tick is newer than the local tick is rejected outright, `forward_slack_ms`
## widens that allowance for clock skew, and a claim older than
## `max_rewind_ms` — converted to ticks through `CYGNetworkTime` — is rejected
## rather than clamped through expired history. Inside the window, per-body
## clamping follows `sample_pose()`.
##
## Claim RPC `cyg_lag_hit`, one `PackedByteArray` argument, byte-exact:
##
##     [shot tick u32 BE]      the tick the shooter rewound against
##     [attacker actor u32 BE] CYGNetwork.local_actor_number of the shooter
##     [target view id i32]    zig-zag varint, `CYGView.view_id` of the victim
##     [origin 3 x f32 BE]     world shot origin
##     [direction 3 x f32 BE]  world shot direction (need not be normalised)
##
## Usage, on the shooter's networked player:
##
##     # player_root (Node3D) > CYGView, CYGLagComp3D, and per body:
##     # enemy_root (CharacterBody3D) > CYGView, CYGLagHitbox3D + CollisionShape3D
##     func fire() -> void:
##         var shot := lag_comp.raycast(muzzle.global_position, aim_dir,
##             lag_comp.render_tick())
##         if shot != null and lag_comp.validate(shot):
##             apply_local_fx(shot.point)
##             lag_comp.report_hit(shot)   # emits the claim RPC to the room
##
## Design derives from Netfox's MIT-licensed rollback and Mirror/UNet's lag
## compensation (attribution in LICENSE-NETFOX.md). Independent GDScript; no
## upstream source text is copied.

## The RPC a claim rides; must match the receiver's `claim_rpc`.
const RPC_CLAIM := "cyg_lag_hit"
## Distance tolerance for a `validate()` re-run; poses lerp, so agreement is
## within a centimetre rather than exact.
const VALIDATE_DISTANCE_EPSILON := 0.01

## Fired by the shooter when `report_hit()` put a validated claim on the wire.
signal hit_reported(target_view_id: int, shot_tick: int, point: Vector3, normal: Vector3)
## Fired on the peer that owns the targeted view when its own recorded history
## agrees the claim ray hit the body at the claimed tick.
signal hit_received(attacker_actor: int, shot_tick: int, point: Vector3, normal: Vector3)
## Fired on the owning peer when its re-check missed: the claim is refused.
signal hit_rejected(attacker_actor: int, shot_tick: int)

## Backward compensation limit, mirroring `MaxRewindWindowSeconds` (0.3 s).
@export var max_rewind_ms: float = 300.0
## Forward slack: how far past the local tick a claim may run before it is
## rejected as "from the future". 0 keeps the reference's hard `now` ceiling.
@export var forward_slack_ms: float = 0.0
## Default ray length when a query passes -1.
@export var max_distance: float = 1000.0
## Collision layers that may block a rewound hit even though they are not
## rewindable themselves. 0 disables the live-space probe and keeps the query
## allocation-free.
@export var occlusion_mask: int = 0
## Per-component override so a 2D and 3D manager can share one view without
## stealing each other's RPC hash slot.
@export var claim_rpc: String = RPC_CLAIM

## The room clock the tick math runs on; `CYGNetwork.network_time` by default,
## injectable for offline rigs and tests.
var time: CYGNetworkTime = null


## The nearest hit a rewind query produced. One instance is reused per manager:
## every `raycast()` overwrites it, so copy the fields you must keep.
class LagHit3D:
	extends RefCounted
	var valid: bool = false
	## The body that was hit — the "target" of the hit.
	var hitbox: CYGLagHitbox3D = null
	## The hitbox's view id, -1 when the body carries no view.
	var view_id: int = -1
	var point: Vector3 = Vector3.ZERO
	var normal: Vector3 = Vector3.ZERO
	var distance: float = 0.0
	## The tick the pose was actually resolved at (after clamping).
	var tick: int = -1
	## The tick the query asked for, after future-clamping.
	var shot_tick: int = -1
	var origin: Vector3 = Vector3.ZERO
	var direction: Vector3 = Vector3.ZERO


var _hit := LagHit3D.new()
var _probe := LagHit3D.new()
var _m_dist: float = 0.0
var _m_normal: Vector3 = Vector3.ZERO

var _warned_no_view: bool = false
var _rpc_bound: bool = false


func _ready() -> void:
	if time == null and is_instance_valid(CYGNetwork):
		time = CYGNetwork.network_time
	# Deferred: a sibling `CYGView` may still be waiting to mint its view id when
	# this component's `_ready` runs; the deferred pass lands after every ready.
	_bind_claim_rpc.call_deferred()


## Spawning may have (re)assigned the view id, so rebind on it.
func cyg_on_spawned() -> void:
	_rpc_bound = false
	_bind_claim_rpc()


## The tick the room is on right now, in the shared clock's numbering.
func current_tick() -> int:
	if time != null:
		return time.tick()
	if is_instance_valid(CYGNetwork):
		return CYGNetwork.current_tick
	return 0


## The tick the shooter is *seeing*: the render time the room's interpolators
## sample at (`CYGNetworkTime.rollback_tick()`), i.e. one latency window back.
## A shot fired from what is on screen should be timestamped here.
func render_tick() -> int:
	if time != null:
		return time.rollback_tick()
	return current_tick()


## Backward window expressed in ticks.
func rewind_ticks() -> int:
	return maxi(1, ceili(max_rewind_ms / _tick_ms()))


## Forward allowance expressed in ticks.
func forward_ticks() -> int:
	return maxi(0, floori(forward_slack_ms / _tick_ms()))


## Rewinds every registered `CYGLagHitbox3D` to the pose it held at `shot_tick`
## and casts `origin + t * direction` against those poses instead of the live
## colliders. `shot_tick` of -1 means "the current tick". Returns the nearest
## hit — target, point, normal, distance and the tick it resolved at — or null
## when nothing was hit or the timestamp falls outside the tolerance windows.
## The returned object is this manager's reusable `_hit`: its contents change
## on the next call. Never moves a live node.
func raycast(origin: Vector3, direction: Vector3, shot_tick: int = -1, query_max: float = -1.0, layer_mask: int = 0, expected_view_id: int = -1) -> LagHit3D:
	if not _raycast_into(_hit, origin, direction, shot_tick, query_max, layer_mask, expected_view_id):
		_hit.valid = false
		return null
	return _hit


## Deterministic accept decision on the shooter's side: re-runs the exact rewind
## the hit claims and requires the same body, the same resolved tick and a
## distance within `VALIDATE_DISTANCE_EPSILON`. Because both runs read the same
## tick-stamped history, agreement never depends on the network.
func validate(hit: LagHit3D) -> bool:
	if hit == null or not hit.valid:
		return false
	# Snap the wanted fields first: re-running clobbers `_hit` when `hit` IS
	# `_hit`, and a validated re-run must agree with the original anyway.
	var want_box := hit.hitbox
	var want_tick := hit.tick
	var want_distance := hit.distance
	var want_view := hit.view_id
	var origin := hit.origin
	var direction := hit.direction
	var shot_tick := hit.shot_tick
	var ok := _raycast_into(_probe, origin, direction, shot_tick, -1.0, 0, want_view)
	if not ok:
		return false
	return _probe.hitbox == want_box \
		and _probe.tick == want_tick \
		and absf(_probe.distance - want_distance) <= VALIDATE_DISTANCE_EPSILON


## Packs the claim and emits it over the RPC path the relay forwards
## (`CYGView.send_rpc`, `CYGOpcodes.Target.ALL`), gated on `validate()`. The
## victim's own peers re-decide locally and answer through `hit_received` /
## `hit_rejected`; the room sees `hit_reported`. False when the claim was not
## validated, not connected, or the transport refused the frame.
func report_hit(hit: LagHit3D = null) -> bool:
	var claim := _hit if hit == null else hit
	if not validate(claim):
		return false
	if not is_instance_valid(CYGNetwork) or not CYGNetwork.is_connected_flag:
		return false
	var target := view
	if target == null:
		return false
	# Read the fields before `_hit` can move again; `claim` may be `_hit`.
	var shot_tick: int = claim.shot_tick
	var view_id: int = claim.view_id
	var origin: Vector3 = claim.origin
	var direction: Vector3 = claim.direction
	var point: Vector3 = claim.point
	var normal: Vector3 = claim.normal
	var writer := CYGBitPacker.new()
	writer.write_uint32_be(shot_tick)
	writer.write_uint32_be(CYGNetwork.local_actor_number)
	writer.write_int32(view_id)
	writer.write_vector3(origin)
	writer.write_vector3(direction)
	var sent := target.send_rpc(claim_rpc, [writer.to_bytes()], CYGOpcodes.Target.ALL)
	if sent:
		hit_reported.emit(view_id, shot_tick, point, normal)
	return sent


## ------------------------------------------------------------------- internals

func _raycast_into(out: LagHit3D, origin: Vector3, direction: Vector3, shot_tick: int, query_max: float, layer_mask: int, expected_view_id: int) -> bool:
	out.valid = false
	if not _ensure_view():
		return false
	var length_sq: float = direction.length_squared()
	if length_sq < 1e-12:
		return false
	var dir: Vector3 = direction / sqrt(length_sq)
	var current := current_tick()
	var query_tick := shot_tick if shot_tick >= 0 else current
	# Forward limit first: a tick from the future is refused, save for whatever
	# `forward_slack_ms` allows, and anything inside the slack clamps to `now`.
	if query_tick > current + forward_ticks():
		return false
	if query_tick > current:
		query_tick = current
	# Backward limit: older than the window is guesswork the references reject
	# outright rather than clamping through evicted history.
	if current - query_tick > rewind_ticks():
		return false
	var limit := query_max if query_max >= 0.0 else max_distance

	var best_box: CYGLagHitbox3D = null
	var best_dist: float = INF
	var best_normal := Vector3.ZERO
	var best_tick := -1
	var self_view_id: int = view.view_id
	var candidates := CYGLagHitbox3D.registry()
	for candidate in candidates:
		var box := candidate as CYGLagHitbox3D
		if box == null or not box.is_queryable():
			continue
		var box_view_id: int = box.get_view_id()
		# A shooter cannot hit its own body; unviewed props (-1) never collide.
		if box_view_id >= 0 and box_view_id == self_view_id:
			continue
		if expected_view_id >= 0 and box_view_id != expected_view_id:
			continue
		if layer_mask != 0 and (layer_mask & box.layer_bits()) == 0:
			continue
		var pose: CYGLagHitbox3D.RewindPose3D = box.sample_pose(query_tick)
		if pose == null:
			continue
		var struck: bool = false
		if pose.is_sphere:
			struck = _ray_sphere(origin, dir, pose.center, pose.extents.x, limit)
		else:
			struck = _ray_aabb(origin, dir, pose.center, pose.extents, limit)
		if struck and _m_dist < best_dist:
			best_box = box
			best_dist = _m_dist
			best_normal = _m_normal
			best_tick = pose.tick

	if best_box == null:
		return false
	if occlusion_mask != 0 and _occluded(origin, origin + dir * best_dist, best_box):
		return false

	out.valid = true
	out.hitbox = best_box
	out.view_id = best_box.get_view_id()
	out.point = origin + dir * best_dist
	out.normal = best_normal
	out.distance = best_dist
	out.tick = best_tick
	out.shot_tick = query_tick
	out.origin = origin
	out.direction = direction
	return true


## Ray-sphere entry, the reference's discriminant form: the near root is the
## hit; a negative near root means "started inside" (c < 0, report t = 0) or
## "shape behind the shooter" (report miss), never a hit at the muzzle.
func _ray_sphere(origin: Vector3, dir: Vector3, center: Vector3, radius: float, limit: float) -> bool:
	var to_center: Vector3 = origin - center
	var along: float = dir.dot(to_center)
	var constant: float = to_center.dot(to_center) - radius * radius
	var discriminant: float = along * along - constant
	if discriminant < 0.0:
		return false
	var dist: float = -along - sqrt(discriminant)
	if dist < 0.0:
		if constant >= 0.0:
			return false
		dist = 0.0
	if dist > limit:
		return false
	_m_dist = dist
	var radial: Vector3 = (origin + dir * dist) - center
	var radial_sq: float = radial.length_squared()
	_m_normal = radial / sqrt(radial_sq) if radial_sq > 1e-12 else -dir
	return true


## Ray/world-AABB entry via the slab method: clip against each axis' near/far
## planes, keep the surviving interval; the axis that last pushed the entry
## plane outward owns the face normal.
func _ray_aabb(origin: Vector3, dir: Vector3, center: Vector3, extents: Vector3, limit: float) -> bool:
	var minimum: Vector3 = center - extents
	var maximum: Vector3 = center + extents
	var entering: float = 0.0
	var leaving: float = limit
	var entry_axis: int = -1
	var entry_from_min: bool = false
	for axis in range(3):
		var o: float = origin[axis]
		var d: float = dir[axis]
		var plane_min: float = minimum[axis]
		var plane_max: float = maximum[axis]
		if absf(d) < 1e-6:
			# Parallel to the slab: inside for good, or never in it.
			if o < plane_min or o > plane_max:
				return false
			continue
		var inv_d: float = 1.0 / d
		var t_near: float = (plane_min - o) * inv_d
		var t_far: float = (plane_max - o) * inv_d
		var near_is_min: bool = t_near < t_far
		if not near_is_min:
			var swap: float = t_near
			t_near = t_far
			t_far = swap
		if t_near > entering:
			entering = t_near
			entry_axis = axis
			entry_from_min = near_is_min
		if t_far < leaving:
			leaving = t_far
		if entering > leaving:
			return false
	if entering > limit:
		return false
	_m_dist = entering
	if entry_axis < 0:
		_m_normal = -dir
	else:
		var face_sign: float = -1.0 if entry_from_min else 1.0
		var normal_axis := Vector3.ZERO
		if entry_axis == 0:
			normal_axis.x = face_sign
		elif entry_axis == 1:
			normal_axis.y = face_sign
		else:
			normal_axis.z = face_sign
		_m_normal = normal_axis
	return true


## Live-space occlusion probe: `PhysicsDirectSpaceState3D` from the viewport's
## `World3D` (it cannot be instantiated). Only runs with `occlusion_mask` set;
## its query object and result Dictionary allocate, which is why the analytic
## rewind path never touches it.
func _occluded(from_point: Vector3, to_point: Vector3, blocked_by: CYGLagHitbox3D) -> bool:
	if not is_inside_tree():
		return false
	var viewport := get_viewport()
	if viewport == null:
		return false
	var world := viewport.find_world_3d()
	if world == null:
		return false
	var query := PhysicsRayQueryParameters3D.create(from_point, to_point, occlusion_mask)
	if blocked_by.is_inside_tree():
		query.exclude = [blocked_by.get_parent()]
	var hit: Dictionary = world.direct_space_state.intersect_ray(query)
	return not hit.is_empty()


func _ensure_view() -> bool:
	var target := view
	if target == null:
		if not _warned_no_view:
			_warned_no_view = true
			push_warning("CYGLagComp3D on %s: no CYGView in the parent chain; lag compensation is a no-op." % name)
		return false
	return true


func _bind_claim_rpc() -> void:
	if _rpc_bound:
		return
	var target := view
	if target == null or target.view_id <= 0:
		return
	target.bind_rpc(claim_rpc, _on_claim_rpc)
	_rpc_bound = true


func _on_claim_rpc(args: Array) -> void:
	# The claim rides the shooter's view; every peer's manager bound there decodes
	# it, but only the peer the target view belongs to owns the true poses.
	if not is_instance_valid(CYGNetwork):
		return
	if args.is_empty() or not (args[0] is PackedByteArray):
		return
	var blob: PackedByteArray = args[0]
	var reader := CYGBitReader.new()
	reader.attach(blob)
	var shot_tick: int = int(reader.read_uint32_be())
	var attacker: int = int(reader.read_uint32_be())
	var target_view_id: int = int(reader.read_int32())
	var origin: Vector3 = reader.read_vector3()
	var direction: Vector3 = reader.read_vector3()
	if reader.last_read_failed:
		return
	var target: CYGView = CYGNetwork.get_view(target_view_id)
	if target == null or not target.is_mine:
		return
	# The veto: re-run the rewind against this peer's own recording of its body.
	var result := raycast(origin, direction, shot_tick, max_distance, 0, target_view_id)
	if result != null and result.valid:
		hit_received.emit(attacker, shot_tick, result.point, result.normal)
	else:
		hit_rejected.emit(attacker, shot_tick)


func _tick_ms() -> float:
	if time != null:
		return time.tick_duration_ms()
	if is_instance_valid(CYGNetwork):
		return 1000.0 / float(maxi(CYGNetwork.tick_rate, 1))
	return 1000.0 / 30.0
