class_name CYGLagComp2D
extends CYGBehaviour
## 2D lag compensation: rewind raycasts against tick-stored collider history,
## then adjudicate the hit peer-to-peer.
##
## The `PhysicsSpace2D` twin of `CYGLagComp3D`, and the same deviation from the
## Unity/Unreal flow: the Rust room relay declares the rewind-hit opcode but
## never forwards it, so lag compensation here is peer-hosted. The shooter
## rewinds the local replicas of every `CYGLagHitbox2D` against the shared
## `CYGNetworkTime` tick numbering, decides the hit with a local deterministic
## rewind (`validate()`), and reports it over `CYGView.send_rpc` — the only
## channel the relay actually delivers. The peer owning the target view re-runs
## the same rewind against its own recorded poses; that local, deterministic
## re-check is the veto (`hit_received` / `hit_rejected`).
##
## Stale replicas are handled by history, not hope: a body whose deltas stopped
## clamps every older query to its newest recorded pose, and a body that never
## received one has no poses to query, so it cannot be hit at all.
##
## Rewinds never move a live node: circles and oriented boxes are solved
## analytically at the recorded pose (position, half extents, rotation angle).
## `World2D.direct_space_state` — reached through the viewport, it cannot be
## instantiated — only runs the optional live-scene occlusion probe when
## `occlusion_mask` is non-zero. With the probe off, a query allocates nothing:
## the hit result, the maths scratch and every sampled pose are reused members.
##
## Tolerance windows match the references: a claim newer than the local tick is
## rejected (widened by `forward_slack_ms` for clock skew), and one older than
## `max_rewind_ms` — 0.3 s by default, expressed in ticks through the clock —
## is rejected rather than clamped through expired history.
##
## Claim RPC `cyg_lag_hit_2d`, one `PackedByteArray` argument, byte-exact:
##
##     [shot tick u32 BE]      the tick the shooter rewound against
##     [attacker actor u32 BE] CYGNetwork.local_actor_number of the shooter
##     [target view id i32]    zig-zag varint, `CYGView.view_id` of the victim
##     [origin 2 x f32 BE]     world shot origin
##     [direction 2 x f32 BE]  world shot direction (need not be normalised)
##
## Usage, on the shooter's networked body:
##
##     # body_root (CharacterBody2D) > CYGView, CYGLagComp2D, and per target:
##     # enemy_root (Area2D) > CYGView, CYGLagHitbox2D + CollisionShape2D
##     func fire() -> void:
##         var shot := lag_comp.raycast(muzzle.global_position, aim_dir,
##             lag_comp.render_tick())
##         if shot != null and lag_comp.validate(shot):
##             lag_comp.report_hit(shot)
##
## Design derives from Netfox's MIT-licensed rollback and Mirror/UNet's lag
## compensation (attribution in LICENSE-NETFOX.md). Independent GDScript; no
## upstream source text is copied.

## The RPC a claim rides; distinct from the 3D claim so one view can host both
## managers without an RPC hash collision.
const RPC_CLAIM := "cyg_lag_hit_2d"
## Distance tolerance for a `validate()` re-run; poses lerp, so agreement is
## within a centimetre rather than exact.
const VALIDATE_DISTANCE_EPSILON := 0.01

## Fired by the shooter when `report_hit()` put a validated claim on the wire.
signal hit_reported(target_view_id: int, shot_tick: int, point: Vector2, normal: Vector2)
## Fired on the peer that owns the targeted view when its own recorded history
## agrees the claim ray hit the body at the claimed tick.
signal hit_received(attacker_actor: int, shot_tick: int, point: Vector2, normal: Vector2)
## Fired on the owning peer when its re-check missed: the claim is refused.
signal hit_rejected(attacker_actor: int, shot_tick: int)

## Backward compensation limit, mirroring `MaxRewindWindowSeconds` (0.3 s).
@export var max_rewind_ms: float = 300.0
## Forward slack for clock skew; 0 keeps the reference's hard `now` ceiling.
@export var forward_slack_ms: float = 0.0
## Default ray length when a query passes -1.
@export var max_distance: float = 1000.0
## Collision layers that may block a rewound hit from the live scene. 0 keeps
## the query on the allocation-free analytic path only.
@export var occlusion_mask: int = 0
## Per-component override for unusual RPC name layouts.
@export var claim_rpc: String = RPC_CLAIM

## The room clock the tick math runs on; `CYGNetwork.network_time` by default,
## injectable for offline rigs and tests.
var time: CYGNetworkTime = null


## The nearest hit a rewind query produced. One instance is reused per manager:
## every `raycast()` overwrites it, so copy the fields you must keep.
class LagHit2D:
	extends RefCounted
	var valid: bool = false
	## The body that was hit — the "target" of the hit.
	var hitbox: CYGLagHitbox2D = null
	## The hitbox's view id, -1 when the body carries no view.
	var view_id: int = -1
	var point: Vector2 = Vector2.ZERO
	var normal: Vector2 = Vector2.ZERO
	var distance: float = 0.0
	## The tick the pose was actually resolved at (after clamping).
	var tick: int = -1
	## The tick the query asked for, after future-clamping.
	var shot_tick: int = -1
	var origin: Vector2 = Vector2.ZERO
	var direction: Vector2 = Vector2.ZERO


var _hit := LagHit2D.new()
var _probe := LagHit2D.new()
var _m_dist: float = 0.0
var _m_normal: Vector2 = Vector2.ZERO

var _warned_no_view: bool = false
var _rpc_bound: bool = false


func _ready() -> void:
	if time == null and is_instance_valid(CYGNetwork):
		time = CYGNetwork.network_time
	# Deferred: a sibling `CYGView` may still be minting its view id when this
	# component's `_ready` runs.
	_bind_claim_rpc.call_deferred()


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


## The tick the shooter is *seeing*: `CYGNetworkTime.rollback_tick()`, one
## latency window back. Shots fired from what is on screen timestamp here.
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


## Rewinds every registered `CYGLagHitbox2D` to its pose at `shot_tick` and
## casts `origin + t * direction` against those poses — circles and oriented
## boxes, angles included — instead of the live colliders. `shot_tick` of -1
## means "the current tick". Returns the nearest hit (target, point, normal,
## distance, resolved tick) or null on a miss or a timestamp outside the
## tolerance windows. The returned object is the manager's reusable `_hit`.
func raycast(origin: Vector2, direction: Vector2, shot_tick: int = -1, query_max: float = -1.0, layer_mask: int = 0, expected_view_id: int = -1) -> LagHit2D:
	if not _raycast_into(_hit, origin, direction, shot_tick, query_max, layer_mask, expected_view_id):
		_hit.valid = false
		return null
	return _hit


## Deterministic accept decision on the shooter's side: re-runs the exact
## rewind and requires the same body, resolved tick, and a distance within
## `VALIDATE_DISTANCE_EPSILON`. Reads the same tick-stamped history, so the
## verdict is reproducible without any network round trip.
func validate(hit: LagHit2D) -> bool:
	if hit == null or not hit.valid:
		return false
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


## Packs the claim and emits it over `CYGView.send_rpc` to `Target.ALL`, gated
## on `validate()`. Only connected peers send; the victim's own peer re-decides
## locally. False when unvalidated, offline, or the transport refused.
func report_hit(hit: LagHit2D = null) -> bool:
	var claim := _hit if hit == null else hit
	if not validate(claim):
		return false
	if not is_instance_valid(CYGNetwork) or not CYGNetwork.is_connected_flag:
		return false
	var target := view
	if target == null:
		return false
	var shot_tick: int = claim.shot_tick
	var view_id: int = claim.view_id
	var origin: Vector2 = claim.origin
	var direction: Vector2 = claim.direction
	var point: Vector2 = claim.point
	var normal: Vector2 = claim.normal
	var writer := CYGBitPacker.new()
	writer.write_uint32_be(shot_tick)
	writer.write_uint32_be(CYGNetwork.local_actor_number)
	writer.write_int32(view_id)
	writer.write_vector2(origin)
	writer.write_vector2(direction)
	var sent := target.send_rpc(claim_rpc, [writer.to_bytes()], CYGOpcodes.Target.ALL)
	if sent:
		hit_reported.emit(view_id, shot_tick, point, normal)
	return sent


## ------------------------------------------------------------------- internals

func _raycast_into(out: LagHit2D, origin: Vector2, direction: Vector2, shot_tick: int, query_max: float, layer_mask: int, expected_view_id: int) -> bool:
	out.valid = false
	if not _ensure_view():
		return false
	var length_sq: float = direction.length_squared()
	if length_sq < 1e-12:
		return false
	var dir: Vector2 = direction / sqrt(length_sq)
	var current := current_tick()
	var query_tick := shot_tick if shot_tick >= 0 else current
	if query_tick > current + forward_ticks():
		return false
	if query_tick > current:
		query_tick = current
	if current - query_tick > rewind_ticks():
		return false
	var limit := query_max if query_max >= 0.0 else max_distance

	var best_box: CYGLagHitbox2D = null
	var best_dist: float = INF
	var best_normal := Vector2.ZERO
	var best_tick := -1
	var self_view_id: int = view.view_id
	var candidates := CYGLagHitbox2D.registry()
	for candidate in candidates:
		var box := candidate as CYGLagHitbox2D
		if box == null or not box.is_queryable():
			continue
		var box_view_id: int = box.get_view_id()
		if box_view_id >= 0 and box_view_id == self_view_id:
			continue
		if expected_view_id >= 0 and box_view_id != expected_view_id:
			continue
		if layer_mask != 0 and (layer_mask & box.layer_bits()) == 0:
			continue
		var pose: CYGLagHitbox2D.RewindPose2D = box.sample_pose(query_tick)
		if pose == null:
			continue
		var struck: bool = false
		if pose.is_circle:
			struck = _ray_circle(origin, dir, pose.center, pose.extents.x, limit)
		else:
			struck = _ray_obb(origin, dir, pose.center, pose.extents, pose.angle, limit)
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


## Ray/circle entry via the discriminant; a negative near root means "started
## inside" (hit at t = 0) only when `constant < 0`, otherwise the body is behind
## the shooter and must not report a muzzle hit.
func _ray_circle(origin: Vector2, dir: Vector2, center: Vector2, radius: float, limit: float) -> bool:
	var to_center: Vector2 = origin - center
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
	var radial: Vector2 = (origin + dir * dist) - center
	var radial_sq: float = radial.length_squared()
	_m_normal = radial / sqrt(radial_sq) if radial_sq > 1e-12 else -dir
	return true


## Ray/oriented-box: rotate the ray into the box's frame (centre and angle),
## run the slab method there, and rotate the entry-face normal back out. This
## is where the 2D rewind carries rotation that a world AABB would lose.
func _ray_obb(origin: Vector2, dir: Vector2, center: Vector2, half: Vector2, angle: float, limit: float) -> bool:
	var local_rot := Transform2D(-angle, Vector2.ZERO)
	var local_origin: Vector2 = local_rot * (origin - center)
	var local_dir: Vector2 = local_rot * dir
	var entering: float = 0.0
	var leaving: float = limit
	var entry_axis: int = -1
	var entry_from_min: bool = false
	for axis in range(2):
		var o: float = local_origin[axis]
		var d: float = local_dir[axis]
		var plane_min: float = -half[axis]
		var plane_max: float = half[axis]
		if absf(d) < 1e-6:
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
	var local_normal := Vector2.ZERO
	if entry_axis < 0:
		local_normal = -local_dir
	else:
		var face_sign: float = -1.0 if entry_from_min else 1.0
		if entry_axis == 0:
			local_normal.x = face_sign
		else:
			local_normal.y = face_sign
	_m_normal = Transform2D(angle, Vector2.ZERO) * local_normal
	return true


## Live-scene occlusion through `World2D.direct_space_state`
## (`intersect_ray`); only consulted when `occlusion_mask` is non-zero, because
## the query object and the result Dictionary allocate. The hit target's own
## body is excluded so a live collider cannot veto its own rewound hit.
func _occluded(from_point: Vector2, to_point: Vector2, blocked_by: CYGLagHitbox2D) -> bool:
	if not is_inside_tree():
		return false
	var viewport := get_viewport()
	if viewport == null:
		return false
	var world := viewport.find_world_2d()
	if world == null:
		return false
	var query := PhysicsRayQueryParameters2D.create(from_point, to_point, occlusion_mask)
	if blocked_by.is_inside_tree():
		query.exclude = [blocked_by.get_parent()]
	var hit: Dictionary = world.direct_space_state.intersect_ray(query)
	return not hit.is_empty()


func _ensure_view() -> bool:
	var target := view
	if target == null:
		if not _warned_no_view:
			_warned_no_view = true
			push_warning("CYGLagComp2D on %s: no CYGView in the parent chain; lag compensation is a no-op." % name)
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
	var origin: Vector2 = reader.read_vector2()
	var direction: Vector2 = reader.read_vector2()
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
