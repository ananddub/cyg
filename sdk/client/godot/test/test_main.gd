extends Node
## Headless validation harness for the CookYourGame Godot SDK.
##
## Parent mode runs two child Godot instances concurrently (alice sends, bob
## receives) against a live server and aggregates their checks. Each child prints
## `CHECK <name> PASS|FAIL` and appends the same line to a per-role report file,
## because `OS.create_process` cannot capture the receiver's stdout.
##
## Run: godot --headless --path sdk/godot                     # full two-client run
##      godot --headless --path sdk/godot ++ role=alice       # one client only

const SERVER_URL := "http://localhost:7777"
const ROOM := "godot_test"
const SCENE_VIEW_ID := 0x00010001
const PROBE_POSE := Vector3(1.25, -3.5, 7.0)
const NATIVE_PACKET_TEXT := "native-packet"
const RUN_SECONDS := 12.0
const MAX_ATTEMPTS := 8
## Phase 6 resilience: alice tears down her own socket here — late enough that every
## other check has already seen traffic, early enough for the SDK's 0.5 s backoff, one
## retry and a full handshake to land before the run ends.
const DROP_AT_SECONDS := 6.5
## The verdict on that outage: by then a working reconnect has joined twice over.
const RECOVERY_VERDICT_SECONDS := 10.0
## Phase 2 avatar: the authority walks it in small steps so the proxy must
## interpolate rather than snap.
const AVATAR_PREFAB_ID := 7
const AVATAR_PREFAB_NAME := "avatar"
const AVATAR_PREFAB_SCENE := "res://test/avatar_prefab.tscn"
const AVATAR_STEP := 0.05
const AVATAR_TRAVEL_MIN := 3.0
const AVATAR_SAMPLES_MIN := 6
## One wire tick moves the avatar 0.05, so a bigger single-frame step means the
## proxy jumped instead of interpolating.
const AVATAR_MAX_FRAME_JUMP := 0.5
## In-process authority/proxy pair for the 2D sync, which needs no relay.
const SYNC2D_VIEW_A := 0x00020001
const SYNC2D_VIEW_B := 0x00020002
const SYNC2D_STEP := 0.05
const SYNC2D_MAX_FRAME_JUMP := 0.2
## Phase 3 rollback: a `CharacterBody3D` whose owner integrates and whose proxies
## predict, spawned far from the avatar so the two streams never overlap.
const ROLLBACK_PREFAB_ID := 8
const ROLLBACK_PREFAB_NAME := "rollback"
const ROLLBACK_PREFAB_SCENE := "res://test/rollback_prefab.tscn"
const ROLLBACK_SPAWN_AT := Vector3(0.0, 0.0, -30.0)
## The proxy predicts at half the authority's speed, so a reconciliation always has
## something to correct: a pass can then never be a lucky agreement. Its velocity may
## only ever leave that ceiling by being dragged onto the authority's trajectory.
const PROXY_PREDICT_SPEED := 3.0
const ROLLBACK_TRACK_MIN := PROXY_PREDICT_SPEED + 0.5
## The authority ramps to `move_speed` 6.0 and never passes it, so anything above
## 6.6 would be a snap rather than a correction.
const ROLLBACK_TRACK_MAX := 6.6
## Predictive rig: a locally registered view, never spawned, driven by its own clock.
const PREDICT_VIEW_ID := 0x00030001
const PREDICT_SNAP_Y := 5.0
const PREDICT_TRUTH_Y := 9.0
## Phase 4 rigs: locally registered views that never reach the relay either.
const TOGGLE_VIEW_ID := 0x00040001
const TOGGLE_PROXY_VIEW_ID := 0x00040002
const DORMANT_VIEW_ID := 0x00040003
const ATTACH_VIEW_ID := 0x00040004
const ATTACH_REMOTE_VIEW_ID := 0x00040005
## The bone a socket publisher claims, shared by both rigs so the wire identity is
## the only difference between them.
const SOCKET_BONE: StringName = &"hand_l"
## The socket owns its slot; the weapon names the same one on the wire.
const SOCKET_SLOT := 3
## Events layer over the relay: a view id both peers register locally, so a one-shot
## RPC from the owner lands on the other side's pre-built pool.
const AUDIO_VIEW_ID := 0x00040006
const AUDIO_POOL_SIZE := 2
## Where the owner says the sound happened. A pooled player only ever sits at its
## own spawn position until an event moves it there, so its position is the evidence
## that the event was played rather than merely received. Nothing else in the harness
## fires at this spot, so a slot here can only have come over the relay.
const AUDIO_EVENT_AT := Vector3(3.5, 1.25, -2.0)
## The two spots the local dedup probe uses, kept away from `AUDIO_EVENT_AT`.
const AUDIO_FIRST_AT := Vector3(1.5, 0.0, 6.5)
const AUDIO_REPEAT_AT := Vector3(-7.5, 0.5, 4.5)
## Written behind a collection blob: reading it back proves the batch consumed
## exactly its own bytes and did not strand the rest of the frame.
const FRAME_MARKER := 0xC0FFEE
## Every check the two clients produce together; the parent fails without any one.
const REQUIRED_CHECKS := [
	"bitpacker_roundtrip", "join_room", "ping_rtt", "connected_flag",
	"rpc_local_echo", "rpc_received", "view_delta", "room_event", "godot_pipe",
	"sync_properties", "actor_spawn", "transform_interpolated",
	"transform_sync_2d", "ownership_toggle",
	"network_clock", "history_window", "rollback_rewind", "tick_interpolation",
	"predictive_rollback", "character_rollback",
	"delta_packer_values", "sync_collections", "authority_toggle_gate",
	"network_dormancy", "bone_attachment_wire",
	"network_audio_dedup", "network_audio_pooled",
	"reconnect_after_drop", "profiler_traffic",
]

var _role := "parent"
var _checks := {}
var _elapsed := 0.0
var _joined := false
var _peer: CYGMultiplayerPeer = null
var _view: CYGView = null
var _probe_sync: ProbeSync = null
var _avatar_view: CYGView = null
var _proxy_view: CYGView = null
var _proxy_samples: Array[float] = []
var _max_frame_jump := 0.0
var _rollback_authority: CYGCharacterSync3D = null
var _rollback_proxy: CYGCharacterSync3D = null
var _predict_holder: Node3D = null
var _predictor: CYGPredictiveSync = null
## High-water marks of the proxy's rewind: the largest correction the ledger ever
## reported and the fastest the body ever went.
var _rollback_max_mispredictions := 0
var _rollback_max_speed := 0.0
## Events-layer rig: the pool is built once in `_ready`, so the only thing a network
## event can do is move one of these existing players.
var _audio_sync: CYGNetworkAudio3D = null
var _audio_pool_built := 0

## Phase 6 resilience: the outage alice inflicts on herself, the reconnects the SDK
## scheduled for it, and the rejoins that ended them.
var _drop_triggered := false
var _reconnect_attempts := 0
var _rejoins := 0
## Counting transport wrapped around alice's client, so the profiler reports the
## traffic a real room run produced rather than numbers a test fed it.
var _profiler_transport: CYGProfilingTransport = null


func _ready() -> void:
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("role="):
			_role = argument.trim_prefix("role=")

	if _role == "parent":
		_run_parent()
		return
	# Connected before the join so the first reconnect of a dropped run is counted.
	CYGNetwork.reconnecting.connect(_on_reconnecting)
	_run_child()


func _on_reconnecting(attempt: int, delay_seconds: float) -> void:
	_reconnect_attempts = maxi(_reconnect_attempts, attempt)
	print("[%s] SDK scheduled reconnect #%d in %.2fs" % [_role, attempt, delay_seconds])


func _run_parent() -> void:
	var attempts := 0
	var result := {}
	# A local machine may run more than one room server behind SO_REUSEPORT on
	# :7777, and the kernel then puts each client in a different process, where
	# no relay can bridge them. Retrying lands both clients on one backend.
	while attempts < MAX_ATTEMPTS:
		attempts += 1
		result = await _run_attempt(attempts)
		if result["failed"] == 0 and result["missing"].is_empty():
			break
		await get_tree().create_timer(1.0).timeout

	print("PARENT RESULT: %d/%d PASS, %d FAIL after %d attempt(s)" % [
		result["passed"], REQUIRED_CHECKS.size(), result["failed"], attempts,
	])
	get_tree().quit(0 if result["failed"] == 0 and result["missing"].is_empty() else 1)


func _run_attempt(attempt: int) -> Dictionary:
	var executable := OS.get_executable_path()
	var project := ProjectSettings.globalize_path("res://")
	for role in ["alice", "bob"]:
		_remove_report(role)

	# OS.execute blocks, so the receiver must run detached: a sequential pair of
	# foreground clients would never share the room at the same moment.
	OS.create_process(executable, PackedStringArray([
		"--headless", "--path", project, "++", "role=bob",
	]), false)
	await get_tree().create_timer(1.0).timeout

	var output := []
	var code := OS.execute(executable, [
		"--headless", "--path", project, "++", "role=alice",
	], output, true)
	var alice_report := str(output[0]) if output.size() > 0 else ""
	var bob_report := await _wait_for_report("bob", 25.0)

	var combined := alice_report + bob_report
	var missing: Array = REQUIRED_CHECKS.filter(
		func(name): return not combined.contains("CHECK %s PASS" % name)
	)
	print("--- attempt %d: alice exit %d, %d PASS, %d FAIL ---" % [
		attempt, code, combined.count("PASS"), combined.count("FAIL"),
	])
	print(alice_report)
	print(bob_report)
	if not missing.is_empty():
		print("attempt %d missing: %s" % [attempt, str(missing)])
	return {
		"failed": combined.count("FAIL"),
		"passed": REQUIRED_CHECKS.size() - missing.size(),
		"missing": missing,
	}


func _wait_for_report(role: String, timeout_seconds: float) -> String:
	var path := _report_path(role)
	var waited := 0.0
	while waited < timeout_seconds:
		await get_tree().create_timer(0.5).timeout
		waited += 0.5
		var text := _read_report(role)
		if text.contains("DONE"):
			return text
	push_warning("harness: no terminal report from %s after %.1fs." % [role, timeout_seconds])
	return _read_report(role)


func _report_path(role: String) -> String:
	return OS.get_temp_dir().path_join("cyg_godot_%s.checks" % role)


func _read_report(role: String) -> String:
	var file := FileAccess.open(_report_path(role), FileAccess.READ)
	return file.get_as_text() if file != null else ""


func _remove_report(role: String) -> void:
	DirAccess.remove_absolute(_report_path(role))


## Lines recorded so far; rewritten wholesale because a detached child cannot
## share an append handle with the parent that reads it.
var _report_lines: PackedStringArray = PackedStringArray()


func _append_report(line: String) -> void:
	_report_lines.append(line)
	var file := FileAccess.open(_report_path(_role), FileAccess.WRITE)
	if file != null:
		file.store_string("\n".join(_report_lines) + "\n")


func _run_child() -> void:
	_remove_report(_role)
	_check("bitpacker_roundtrip", _test_bitpacker())
	_check("sync_properties", _test_property_map())
	_check("network_clock", _test_network_clock())
	_check("history_window", _test_history_buffer())
	_check("rollback_rewind", _test_rollback_synchronizer())
	_check("tick_interpolation", _test_tick_interpolation())
	_check("delta_packer_values", _test_delta_packer_values())
	_check("sync_collections", _test_sync_collections())
	_install_prefabs()

	var network := get_node("/root/CYGNetwork")
	var settings := CYGServerSettings.new()
	settings.server_http_url = SERVER_URL
	network.settings = settings
	network.joined_room.connect(_on_joined)
	network.connect_to_room(ROOM, _role)


## Registers the networked prefabs from their `.tscn` files, the editor-authored
## shape a game ships: the avatar is `CYGActor3D` + `CYGView` + `CYGTransformSync3D`
## + `CYGOwnershipToggle`, the rollback body is `CharacterBody3D` + `CYGView` +
## `CYGCharacterSync3D` in remote-authority mode.
func _install_prefabs() -> void:
	var registry := CYGPrefabRegistry.new()
	registry.entries = [
		_prefab_entry(AVATAR_PREFAB_ID, AVATAR_PREFAB_NAME, AVATAR_PREFAB_SCENE),
		_prefab_entry(ROLLBACK_PREFAB_ID, ROLLBACK_PREFAB_NAME, ROLLBACK_PREFAB_SCENE),
	]
	CYGNetwork.prefabs = registry


func _prefab_entry(id: int, prefab_name: String, scene_path: String) -> CYGPrefabEntry:
	var entry := CYGPrefabEntry.new()
	entry.id = id
	entry.name = prefab_name
	entry.scene = load(scene_path) as PackedScene
	if entry.scene == null:
		push_error("harness: %s did not load as a PackedScene." % scene_path)
	return entry


## Drift behaviour of the rollback clock: one tick of disagreement is frame jitter and
## eases, two means the counter moved and re-anchors outright.
func _test_network_clock() -> bool:
	var clock := CYGNetworkTime.new()
	clock.ticks_per_second = 30
	clock.observe_tick(10)
	clock.advance(clock.tick_duration_ms() + 0.5)
	var counted := clock.tick() == 11
	clock.latency_ms = 100.0
	var lagged := clock.latency_ticks() == 3 and clock.rollback_tick() == 8
	var phased := clock.sub_tick_phase() >= 0.0 and clock.sub_tick_phase() <= 1.0
	# A single tick of drift only moves the smoothing offset; the counter stays put, so
	# one late frame cannot teleport the prediction timeline.
	clock.observe_tick(12)
	var eased := absf(clock.smoothing_offset_ms()) > 0.5 and clock.tick() == 11
	clock.advance(clock.tick_duration_ms())
	var caught_up := clock.tick() == 12
	clock.observe_tick(30)
	var snapped := clock.tick() == 30 and clock.smoothing_offset_ms() == 0.0
	return counted and lagged and phased and eased and caught_up and snapped


## Slot-map eviction: a write past capacity silently ages out the oldest tick, and the
## cached extremes survive out-of-order writes.
func _test_history_buffer() -> bool:
	var buffer := CYGHistoryBuffer.new(8)
	for i in range(8):
		buffer.push(i, i * 2)
	buffer.push(8, 16)
	var ringed: bool = buffer.capacity() == 8 and buffer.size() == 8 \
		and buffer.get_state(8) == 16 and buffer.get_state(0) == null \
		and buffer.oldest_tick() == 1 and buffer.newest_tick() == 8

	buffer.erase_through(3)
	var erased := buffer.size() == 5 and buffer.oldest_tick() == 4 and not buffer.contains(3)

	buffer.trim_to(2)
	var trimmed := buffer.size() == 2 and buffer.oldest_tick() == 7 and buffer.newest_tick() == 8

	buffer.push(9, 18)
	buffer.push(6, 12)
	var reordered := buffer.oldest_tick() == 6 and buffer.newest_tick() == 9

	buffer.erase_through(9)
	var emptied := buffer.size() == 0 and buffer.newest_tick() == -1
	return ringed and erased and trimmed and reordered and emptied


## Deterministic resimulation: a wrong prediction must be rewound and replayed onto the
## authority's trajectory, and an already-agreeing snapshot must move nothing.
func _test_rollback_synchronizer() -> bool:
	var dummy := RollbackDummy.new()
	var clock := CYGNetworkTime.new()
	clock.ticks_per_second = 30
	clock.observe_tick(0)
	var synchronizer := CYGRollbackSynchronizer.new(clock)
	synchronizer.register(1, dummy.save_state, dummy.restore_state, dummy.simulate)
	# A lambda captures the enclosing locals by value, so what the signal reports has to
	# live in a reference type for the assertions to see it.
	var emits := {"rewinds": 0, "landed_to": -1}
	synchronizer.reconciled.connect(
		func(_id: int, _from: int, to_tick: int, _mis: int) -> void:
			emits["rewinds"] += 1
			emits["landed_to"] = to_tick)

	for _i in range(6):
		clock.advance(clock.tick_duration_ms() + 0.5)
		synchronizer.predict(1, 1.0)
	var predicted: bool = dummy.position_x == 6.0 and synchronizer.pending_input_count(1) == 6

	# The authority had travelled 30 units by the tick it confirms, three times what
	# we recorded: everything newer has to be replayed on top of its state.
	var corrected := synchronizer.reconcile(1, 3, {"position_x": 30.0, "velocity": 1.0})
	var rewound: bool = corrected == 3 and dummy.position_x == 33.0 \
		and emits["landed_to"] == 6 and emits["rewinds"] == 1 \
		and synchronizer.pending_input_count(1) == 3
	var misreported := synchronizer.misprediction_count(1) == 3

	synchronizer.acknowledge(1, 6)
	var settled: bool = synchronizer.pending_input_count(1) == 0 \
		and synchronizer.last_authority_tick(1) == 6

	clock.advance(clock.tick_duration_ms() + 0.5)
	synchronizer.predict(1, 1.0)
	var agreeing := synchronizer.reconcile(1, 7, {"position_x": 34.0, "velocity": 1.0}) == 0
	var ok: bool = predicted and rewound and misreported and settled and agreeing \
		and dummy.position_x == 34.0 and emits["rewinds"] == 1
	if not ok:
		var state := "predicted=%s rewound=%s mis=%s settled=%s agreeing=%s" % [
			predicted, rewound, misreported, settled, agreeing]
		print("[%s] rollback unit diag: %s corrected=%d x=%.3f emits=%s tick=%d" % [
			_role, state, corrected, dummy.position_x, str(emits), clock.tick(),
		])
	return ok


## Interpolation at the rollback render time, i.e. one latency window in the past, plus
## the hold that a zero extrapolation limit must fall back to.
func _test_tick_interpolation() -> bool:
	var clock := CYGNetworkTime.new()
	clock.ticks_per_second = 30
	clock.observe_tick(0)
	var interpolator := CYGTickInterpolator.new(clock)
	interpolator.create_float_track("speed")
	interpolator.create_vector3_track("aim")
	for _i in range(5):
		clock.advance(clock.tick_duration_ms() + 0.5)
		var value_tick := clock.tick()
		interpolator.push_float("speed", value_tick, float(value_tick) * 2.0)
		interpolator.push_vector3("aim", value_tick, Vector3(float(value_tick), 0.0, 0.0))

	clock.latency_ms = 66.7
	var speed: float = interpolator.sample_float("speed")
	var aim: Vector3 = interpolator.sample_vector3("aim")
	var bracketed: bool = absf(speed - 6.0) < 0.001 and aim.is_equal_approx(Vector3(3, 0, 0)) \
		and interpolator.last_value_float("speed") == 10.0 \
		and absf(interpolator.tick_diff_float("speed") - 2.0) < 0.001
	var unknown: bool = interpolator.sample_float("missing") == null \
		and interpolator.track_keys().size() == 2

	interpolator.set_extrapolation_limit(0)
	clock.advance(clock.tick_duration_ms() * 10.0)
	var held: float = interpolator.sample_float("speed")
	return bracketed and unknown and absf(held - 10.0) < 0.001


## A predictor built in-process with a clock only this test advances, so the ledger's
## ticks are exact numbers rather than whatever the room clock read at the moment of
## the call. The first authority answer is adopted outright (nothing recorded under a
## different numbering may honestly be compared with it); the second has to rewind.
func _run_predictive_rollback() -> void:
	var clock := CYGNetworkTime.new()
	clock.ticks_per_second = CYGNetwork.tick_rate
	clock.observe_tick(0)

	var holder := Node3D.new()
	holder.name = "PredictRig"
	add_child(holder)
	_predict_holder = holder
	var view := CYGView.new()
	view.name = "CYGView"
	view.view_id = PREDICT_VIEW_ID + CYGNetwork.local_actor_number
	# A foreign owner makes this peer a predictor instead of the adjudicator, and an
	# adjudicator has nothing to rewind: its own state is the definition of correct.
	view.owner_actor = CYGNetwork.local_actor_number + 7777
	holder.add_child(view)

	var predictor := CYGPredictiveSync.new()
	predictor.name = "Predictor"
	# Handed in before `add_child` so `_ready` keeps it instead of taking the room clock.
	predictor.time = clock
	predictor.authority_target = CYGOpcodes.Target.OWNER
	holder.add_child(predictor)
	predictor.configure(_predict_move)
	_predictor = predictor

	predictor.apply_authority(clock.tick(), [Vector3(0.0, PREDICT_SNAP_Y, 0.0), 2.0])
	var snapped: bool = is_equal_approx(holder.global_position.y, PREDICT_SNAP_Y) \
		and is_zero_approx(holder.global_position.z) and predictor.sim_speed == 2.0

	# Two predictions at the wrong speed, then the authority's answer for the first of
	# them: the pose must come back and only the newer local intent survive it.
	clock.advance(clock.tick_duration_ms() + 0.5)
	predictor.predict_input(3.0)
	var first_tick := clock.tick()
	clock.advance(clock.tick_duration_ms() + 0.5)
	predictor.predict_input(4.0)
	predictor.apply_authority(first_tick, [Vector3(0.0, PREDICT_TRUTH_Y, 1.0), 2.0])

	var step_seconds: float = clock.tick_duration_ms() / 1000.0
	var expected_z: float = 1.0 + 4.0 * step_seconds
	var rewound: bool = is_equal_approx(holder.global_position.y, PREDICT_TRUTH_Y) \
		and absf(holder.global_position.z - expected_z) < 0.001 \
		and predictor.sim_speed == 4.0
	_check("predictive_rollback", snapped and rewound)
	if not (snapped and rewound):
		print("[%s] predict diag: snapped=%s rewound=%s pos=%s want_z=%.5f speed=%s tick=%d" % [
			_role, snapped, rewound, holder.global_position, expected_z,
			predictor.sim_speed, clock.tick(),
		])
	CYGNetwork.unregister_view(view.view_id)


## The predictive rig's integrator: `speed` is the state the ledger snapshots, so a
## rewind re-derives the position from the restored speed and the replayed inputs.
func _predict_move(input: Variant, delta_ms: float) -> void:
	var speed := float(input)
	_predictor.sim_speed = speed
	_predict_holder.global_position += Vector3(0.0, 0.0, speed * delta_ms / 1000.0)


## Spawns the rollback body once both peers are in the room and holds its input down,
## so every network tick the authority integrates and ships one `cyg_input` fan-out
## plus a tick-stamped snapshot.
func _run_rollback_authority() -> void:
	# Past the avatar (2 s) so the two prefabs are never in flight at once.
	await get_tree().create_timer(5.0).timeout
	if not CYGNetwork.is_connected_flag:
		return
	var view := CYGNetwork.spawn_prefab(ROLLBACK_PREFAB_NAME, ROLLBACK_SPAWN_AT)
	var actor: CharacterBody3D = null
	if view != null:
		actor = view.network_node as CharacterBody3D
	if actor != null:
		_rollback_authority = actor.get_node_or_null("CharacterSync") as CYGCharacterSync3D
	if _rollback_authority == null:
		print("[alice] rollback diag: no CYGCharacterSync3D on the spawned body (view=%s)" % view)
		_check("character_rollback", false)
		return
	CYGNetwork.network_tick.connect(_drive_rollback_authority)


func _drive_rollback_authority(_tick: int) -> void:
	if _rollback_authority == null or not is_instance_valid(_rollback_authority):
		return
	# The proxy predicts at half this speed, so every snapshot it receives disagrees.
	_rollback_authority.set_input(Vector2(1.0, 0.0), false)


## Claims the second spawn for the rollback rig instead of the avatar assertion.
func _setup_rollback_proxy(actor: Node3D) -> void:
	var sync := actor.get_node_or_null("CharacterSync") as CYGCharacterSync3D
	if sync == null:
		return
	# Deliberately slower than the authority: an agreement here could only come from
	# the correction, never from the proxy's own integrator.
	sync.move_speed = PROXY_PREDICT_SPEED
	_rollback_proxy = sync


## Watches the proxy's corrected velocity: a rewind must have happened and the body
## must have been dragged past what its own input could produce. A snap past the
## authority's ceiling is reported immediately instead of waiting out the run.
func _sample_rollback_proxy() -> void:
	if _checks.has("character_rollback"):
		return
	if _rollback_proxy == null or not is_instance_valid(_rollback_proxy):
		return
	_rollback_max_mispredictions = maxi(
		_rollback_max_mispredictions, _rollback_proxy.misprediction_count())
	_rollback_max_speed = maxf(_rollback_max_speed, _rollback_proxy.velocity.x)
	if _rollback_max_speed > ROLLBACK_TRACK_MAX:
		print("[bob] rollback diag: velocity %.3f snapped past the authority ceiling."
			% _rollback_max_speed)
		_check("character_rollback", false)
		return
	if _rollback_max_mispredictions == 0 or _rollback_max_speed < ROLLBACK_TRACK_MIN:
		return
	_check("character_rollback", true)


## Local round trip of the sync-property block: registration order is the wire
## identity, so the reader must land on the same names, apply only the dirty
## entries and fire the change hook for each of them.
func _test_property_map() -> bool:
	var writer := CYGBitPacker.new()
	var sent := CYGPropertyMap.new()
	sent.register("health", 100)
	sent.register("label", "player")
	sent.register("aim", Vector3(1.5, -2.0, 3.25))
	sent.register("flag", true)
	sent.set_value("health", 42)
	sent.set_value("label", "solo")
	sent.set_value("aim", Vector3(-4.5, 0.5, 2.0))
	sent.set_value("flag", false)
	sent.serialize(writer)

	var reader := CYGBitReader.new()
	reader.attach(writer.to_bytes())
	# The change hook belongs to the peer that APPLIES a delta, not the one that
	# wrote it.
	var changed: Array = []
	var received := CYGPropertyMap.new()
	received.register("health", 0, func(name: String, _old: Variant, _value: Variant) -> void:
		changed.append(name))
	received.register("label", "", func(name: String, _old: Variant, _value: Variant) -> void:
		changed.append(name))
	received.register("aim", Vector3.INF, func(name: String, _old: Variant, _value: Variant) -> void:
		changed.append(name))
	received.register("flag", true, func(name: String, _old: Variant, _value: Variant) -> void:
		changed.append(name))
	received.deserialize(reader)

	var ok: bool = received.get_value("health") == 42 \
		and received.get_value("label") == "solo" \
		and received.get_value("aim") == Vector3(-4.5, 0.5, 2.0) \
		and received.get_value("flag") == false
	# Every dirty property must reach the callback, in registration order.
	return ok and changed.size() == 4 and str(changed[3]) == "flag"


## The tagged codec is what lets a networked collection hold more than scalars, and
## the mask is what lets it ship only the slots that moved: a nested value that came
## back with the wrong shape, or a mask whose set bits the receiver cannot walk,
## would corrupt every blob behind it in the same frame.
func _test_delta_packer_values() -> bool:
	var writer := CYGBitPacker.new()
	CYGDeltaPacker.write_value(writer, null)
	CYGDeltaPacker.write_value(writer, [1, "two", Vector3(3.0, 4.0, 5.0), {"nested": 6.5}])
	CYGDeltaPacker.write_value(writer, Color(0.25, 0.5, 0.75, 1.0))
	CYGDeltaPacker.write_value(writer, PackedByteArray([7, 8, 9]))
	var reader := CYGBitReader.new()
	reader.attach(writer.to_bytes())
	var nil_ok: bool = CYGDeltaPacker.read_value(reader) == null
	var list: Variant = CYGDeltaPacker.read_value(reader)
	var nested_ok: bool = list is Array and (list as Array).size() == 4 \
		and list[0] == 1 and list[1] == "two" and list[2] == Vector3(3.0, 4.0, 5.0) \
		and (list[3] as Dictionary).get("nested") == 6.5
	var color_ok: bool = CYGDeltaPacker.read_value(reader) == Color(0.25, 0.5, 0.75, 1.0)
	var bytes: Variant = CYGDeltaPacker.read_value(reader)
	var bytes_ok: bool = bytes is PackedByteArray and (bytes as PackedByteArray) == PackedByteArray([7, 8, 9])

	var mask := CYGDeltaPacker.new()
	mask.configure(20)
	mask.mark(0)
	mask.mark(7)
	mask.mark(8)
	mask.mark(19)
	var mask_writer := CYGBitPacker.new()
	mask.write_mask(mask_writer)
	var received := CYGDeltaPacker.new()
	var mask_reader := CYGBitReader.new()
	mask_reader.attach(mask_writer.to_bytes())
	var walked: Array[int] = []
	if received.read_mask(mask_reader):
		var slot := received.first_marked()
		while slot >= 0:
			walked.append(slot)
			slot = received.next_marked(slot)
	# Twenty slots is three bytes, and the two bits the last byte does not own must
	# not be counted as changes.
	var mask_ok: bool = received.slot_count() == 20 and received.marked_count() == 4 \
		and walked.size() == 4 and walked[0] == 0 and walked[1] == 7 and walked[2] == 8 \
		and walked[3] == 19
	return nil_ok and nested_ok and color_ok and bytes_ok and mask_ok


## Element-level diffs, the revision that orders them, and the full state that
## repairs a gap. Every blob is followed by a marker in the same buffer, so a batch
## that failed to consume exactly its own bytes cannot pass.
func _test_sync_collections() -> bool:
	var array_events := {"added": 0, "updated": 0, "removed": 0}
	var authority := CYGSyncArray.new()
	authority.append(7)
	authority.append("slot")
	authority.insert(1, Vector3(1.0, 2.0, 3.0))
	authority.set_element(0, 11)
	authority.remove_at(2)
	var remote := CYGSyncArray.new()
	remote.element_added.connect(func(_index: int, _value: Variant) -> void:
		array_events["added"] += 1)
	remote.element_updated.connect(func(_index: int, _old: Variant, _value: Variant) -> void:
		array_events["updated"] += 1)
	remote.element_removed.connect(func(_index: int, _value: Variant) -> void:
		array_events["removed"] += 1)

	var writer := CYGBitPacker.new()
	authority.serialize(writer)
	writer.write_uint32(FRAME_MARKER)
	var reader := CYGBitReader.new()
	reader.attach(writer.to_bytes())
	var applied: bool = remote.deserialize(reader)
	var landed: bool = remote.size() == 2 and remote.at(0) == 11 \
		and remote.at(1) == Vector3(1.0, 2.0, 3.0)
	var aligned: bool = reader.read_uint32() == FRAME_MARKER
	var events_ok: bool = int(array_events["added"]) == 3 and int(array_events["updated"]) == 1 \
		and int(array_events["removed"]) == 1

	# Two more mutations, then the batch that carries the second of them is delivered
	# alone: the receiver is one revision short, so it must refuse the batch without
	# applying it and ask for the list instead of guessing a half state.
	authority.set_element(0, 21)
	authority.append(99)
	var lost := CYGBitPacker.new()
	authority.serialize(lost)
	authority.append(100)
	var jumped := CYGBitPacker.new()
	authority.serialize(jumped)
	jumped.write_uint32(FRAME_MARKER)
	var gap_reader := CYGBitReader.new()
	gap_reader.attach(jumped.to_bytes())
	var refused: bool = not remote.deserialize(gap_reader) and remote.needs_snapshot()
	var gap_aligned: bool = gap_reader.read_uint32() == FRAME_MARKER
	var untouched: bool = remote.size() == 2 and remote.at(0) == 11

	var repair := CYGBitPacker.new()
	authority.write_full_state(repair)
	var repair_reader := CYGBitReader.new()
	repair_reader.attach(repair.to_bytes())
	var repaired: bool = remote.deserialize(repair_reader) and not remote.needs_snapshot() \
		and remote.size() == 4 and remote.at(0) == 21 \
		and remote.at(1) == Vector3(1.0, 2.0, 3.0) and remote.at(2) == 99 and remote.at(3) == 100

	var dict := CYGSyncDict.new()
	dict.set_value("ammo", 30)
	dict.set_value("mode", "quest")
	dict.set_value("ammo", 12)
	dict.remove("mode")
	dict.set_value("shield", 4.5)
	var dict_remote := CYGSyncDict.new()
	var dict_writer := CYGBitPacker.new()
	dict.serialize(dict_writer)
	dict_writer.write_uint32(FRAME_MARKER)
	var dict_reader := CYGBitReader.new()
	dict_reader.attach(dict_writer.to_bytes())
	var dict_applied: bool = dict_remote.deserialize(dict_reader)
	var dict_keys := dict_remote.sorted_keys()
	var dict_ok: bool = dict_applied and dict_remote.size() == 2 \
		and dict_remote.get_value("ammo") == 12 and not dict_remote.has("mode") \
		and is_equal_approx(float(dict_remote.get_value("shield")), 4.5) \
		and dict_keys.size() == 2 and str(dict_keys[0]) == "ammo" and str(dict_keys[1]) == "shield" \
		and dict_reader.read_uint32() == FRAME_MARKER

	# A property a peer merely RECEIVED is that peer's new baseline, not its own
	# intent: leaving it dirty would have every proxy re-forward what it was told.
	var health := CYGSyncProperty.new("health", 100)
	var mirror := CYGSyncProperty.new("health", 0)
	var mirror_seen := {"count": 0}
	mirror.value_changed.connect(func(_name: String, _old: Variant, _value: Variant) -> void:
		mirror_seen["count"] += 1)
	var was_clean: bool = not health.has_delta()
	health.set_value(42)
	var sender_dirty: bool = health.has_delta()
	var property_writer := CYGBitPacker.new()
	health.serialize(property_writer)
	property_writer.write_uint32(FRAME_MARKER)
	var property_reader := CYGBitReader.new()
	property_reader.attach(property_writer.to_bytes())
	var prop_deserialized: bool = mirror.deserialize(property_reader)
	var prop_value_ok: bool = int(mirror.get_value()) == 42
	var prop_signal_ok: bool = int(mirror_seen["count"]) == 1
	var prop_clean_after: bool = not mirror.has_delta()
	var prop_aligned: bool = property_reader.read_uint32() == FRAME_MARKER
	# `serialize()` consumes the writer's own dirty flag: after a write the sender is
	# back to its baseline, so the next tick carries nothing unless something moves.
	var property_ok: bool = was_clean and sender_dirty and not health.has_delta() \
		and prop_deserialized and prop_value_ok and prop_signal_ok and prop_clean_after and prop_aligned

	# A clean write still emits its presence byte, so the frame layout never depends
	# on which peer happened to be dirty this tick.
	var quiet := CYGBitPacker.new()
	mirror.serialize(quiet)
	var quiet_reader := CYGBitReader.new()
	quiet_reader.attach(quiet.to_bytes())
	var quiet_ok: bool = quiet.to_bytes().size() == 1 and mirror.deserialize(quiet_reader)

	var array_ok: bool = applied and landed and aligned and events_ok \
		and refused and gap_aligned and untouched and repaired
	return array_ok and dict_ok and property_ok and quiet_ok


## Builds a `Node3D` + `CYGView` + `CYGAuthorityToggle` rig whose gated children
## already exist, so the toggle's first `apply()` sees the whole node path.
func _make_toggle_rig(view_id: int, owner: int) -> Dictionary:
	var holder := Node3D.new()
	holder.name = "ToggleRig%d" % view_id
	add_child(holder)
	var view := CYGView.new()
	view.name = "CYGView"
	view.view_id = view_id
	view.owner_actor = owner
	holder.add_child(view)
	var gated := Node3D.new()
	gated.name = "Gated"
	holder.add_child(gated)
	var inverted := Node3D.new()
	inverted.name = "Inverted"
	holder.add_child(inverted)
	var toggle := CYGAuthorityToggle.new()
	toggle.name = "Toggle"
	var enabled: Array[NodePath] = [NodePath("../Gated")]
	var disabled: Array[NodePath] = [NodePath("../Inverted")]
	toggle.enable_when_active = enabled
	toggle.disable_when_active = disabled
	holder.add_child(toggle)
	return {"node": holder, "view": view, "toggle": toggle, "gated": gated, "inverted": inverted}


## The three gates, read on a view this peer owns and on one it does not, plus the
## effect the gate has on the nodes it names. A toggle that lit on every peer would
## run the room's logic once per client.
func _test_authority_toggle_gate() -> bool:
	var local_actor: int = CYGNetwork.local_actor_number
	var mine := _make_toggle_rig(TOGGLE_VIEW_ID, local_actor)
	var theirs := _make_toggle_rig(TOGGLE_PROXY_VIEW_ID, local_actor + 7777)
	CYGNetwork.unregister_view(TOGGLE_VIEW_ID)
	CYGNetwork.unregister_view(TOGGLE_PROXY_VIEW_ID)

	var toggle: CYGAuthorityToggle = mine["toggle"]
	var other: CYGAuthorityToggle = theirs["toggle"]
	toggle.gate = CYGAuthorityToggle.Gate.OWNER_ONLY
	other.gate = CYGAuthorityToggle.Gate.OWNER_ONLY
	var owner_only: bool = toggle.is_active and not other.is_active

	toggle.gate = CYGAuthorityToggle.Gate.NOT_OWNER
	other.gate = CYGAuthorityToggle.Gate.NOT_OWNER
	var not_owner: bool = not toggle.is_active and other.is_active

	toggle.gate = CYGAuthorityToggle.Gate.MASTER_CLIENT_ONLY
	var was_master: bool = CYGNetwork.local_player.is_master_client
	CYGNetwork.local_player.is_master_client = true
	var master_on: bool = toggle.is_active
	CYGNetwork.local_player.is_master_client = false
	var master_off: bool = not toggle.is_active
	CYGNetwork.local_player.is_master_client = was_master

	# Gating means the named nodes stop running, not that they are merely hidden.
	var gated: Node3D = mine["gated"]
	var inverted: Node3D = mine["inverted"]
	toggle.gate = CYGAuthorityToggle.Gate.NOT_OWNER
	toggle.apply()
	var off_ok: bool = not gated.is_processing() and inverted.is_processing()
	toggle.gate = CYGAuthorityToggle.Gate.OWNER_ONLY
	toggle.apply()
	var on_ok: bool = gated.is_processing() and not inverted.is_processing()

	# Zero wire bytes in both directions: the gate is a local decision, and an
	# observable that writes nothing must read nothing.
	var bytes_out := CYGBitPacker.new()
	toggle.serialize(bytes_out)
	var empty_reader := CYGBitReader.new()
	empty_reader.attach(bytes_out.to_bytes())
	toggle.deserialize(empty_reader)
	var silent: bool = bytes_out.to_bytes().is_empty() and not toggle.has_delta()

	mine["node"].queue_free()
	theirs["node"].queue_free()
	return owner_only and not_owner and master_on and master_off and off_ok and on_ok and silent


## Out of interest range a view must cost the tick pump nothing at all, and state
## that changed while culled has to stay queued: an observer that just found the
## object on screen needs one full refresh, not a resume from a world it never saw.
func _run_dormancy_rig() -> void:
	var holder := Node3D.new()
	holder.name = "DormantRig"
	add_child(holder)
	var view := CYGView.new()
	view.name = "CYGView"
	view.view_id = DORMANT_VIEW_ID
	view.owner_actor = CYGNetwork.local_actor_number
	holder.add_child(view)
	# Out of the table so the run's own delta pump cannot consume what this rig moves
	# by hand; the dormancy flag it sets is still the flag the pump reads.
	CYGNetwork.unregister_view(DORMANT_VIEW_ID)

	var observer := Node3D.new()
	observer.name = "Observer"
	holder.add_child(observer)
	var cull := CYGVisibilityCull.new()
	cull.name = "Cull"
	cull.observer_path = NodePath("../Observer")
	cull.cull_distance = 4.0
	cull.hysteresis = 1.0
	holder.add_child(cull)
	cull.register_property("score", 0)

	observer.position = Vector3(0.0, 0.0, 20.0)
	# The rule is deliberately slow-moving, so give it two evaluate intervals.
	await get_tree().create_timer(0.8).timeout
	var culled: bool = cull.is_dormant and view.network_dormant and not cull.in_range
	cull.set_property("score", 7)
	# Dormant clamps this component's own delta while the write stays queued.
	var held: bool = not cull.has_delta() and cull.properties.dirty_count() == 1

	observer.position = Vector3(0.0, 0.0, 1.0)
	await get_tree().create_timer(0.8).timeout
	var woken: bool = not cull.is_dormant and not view.network_dormant and cull.in_range \
		and cull.has_delta() and cull.properties.dirty_count() == 1

	var passed: bool = culled and held and woken
	_check("network_dormancy", passed)
	if not passed:
		print("[%s] dormancy diag: culled=%s held=%s woken=%s dormant=%s flag=%s in_range=%s dirty=%d" % [
			_role, culled, held, woken, cull.is_dormant, view.network_dormant,
			cull.in_range, cull.properties.dirty_count(),
		])
	holder.queue_free()


## Builds an avatar (a `Skeleton3D` with a hand bone and a socket publisher) plus a
## weapon whose view belongs to `owner`.
func _make_socket_rig(view_id: int, owner: int, tag: String) -> Dictionary:
	var avatar := Node3D.new()
	avatar.name = "Avatar%s" % tag
	add_child(avatar)
	var avatar_view := CYGView.new()
	avatar_view.name = "CYGView"
	avatar_view.view_id = view_id + 10
	avatar_view.owner_actor = owner
	avatar.add_child(avatar_view)
	var skeleton := Skeleton3D.new()
	skeleton.name = "Skeleton"
	avatar.add_child(skeleton)
	var bone := skeleton.add_bone(String(SOCKET_BONE))
	skeleton.set_bone_pose_position(bone, Vector3(0.0, 1.2, -0.3))
	var socket := BoneAttachment3D.new()
	socket.name = "Socket%s" % tag
	socket.bone_name = SOCKET_BONE
	skeleton.add_child(socket)
	var provider := CYGBoneAttachmentSync.new()
	provider.name = "Provider"
	provider.role = CYGBoneAttachmentSync.Role.SOCKET_PROVIDER
	# The hand publishes the slot weapons equip into; the sender names the same slot
	# on the wire, so the keys meet.
	provider.socket_slot = SOCKET_SLOT
	socket.add_child(provider)

	var weapon := Node3D.new()
	weapon.name = "Weapon%s" % tag
	add_child(weapon)
	var weapon_view := CYGView.new()
	weapon_view.name = "CYGView"
	weapon_view.view_id = view_id
	weapon_view.owner_actor = owner
	weapon.add_child(weapon_view)
	var sync := CYGBoneAttachmentSync.new()
	sync.name = "Attachment"
	sync.role = CYGBoneAttachmentSync.Role.ATTACHMENT
	weapon.add_child(sync)
	return {
		"avatar": avatar, "weapon": weapon, "socket": socket,
		"view": weapon_view, "sync": sync,
	}


## The socket's identity and the authored offset are the whole payload: the bone
## transform itself never crosses the wire, because each peer's own skeleton puts
## the weapon where its own copy of the hand currently is.
func _run_bone_attachment_rig() -> void:
	var local_actor: int = CYGNetwork.local_actor_number
	var foreign := local_actor + 7777
	var carrier := _make_socket_rig(ATTACH_VIEW_ID, local_actor, "Carrier")
	var watcher := _make_socket_rig(ATTACH_REMOTE_VIEW_ID, foreign, "Watcher")
	CYGNetwork.unregister_view(ATTACH_VIEW_ID)
	CYGNetwork.unregister_view(ATTACH_VIEW_ID + 10)
	CYGNetwork.unregister_view(ATTACH_REMOTE_VIEW_ID)
	CYGNetwork.unregister_view(ATTACH_REMOTE_VIEW_ID + 10)

	var sender: CYGBoneAttachmentSync = carrier["sync"]
	var receiver: CYGBoneAttachmentSync = watcher["sync"]

	# Nothing dirty writes exactly one zero byte, so a peer that changed nothing still
	# leaves the shared reader on the same boundary as its writer.
	var quiet := CYGBitPacker.new()
	sender.serialize(quiet)
	var quiet_bytes := quiet.to_bytes()
	var quiet_reader := CYGBitReader.new()
	quiet_reader.attach(quiet_bytes)
	receiver.deserialize(quiet_reader)
	var quiet_ok: bool = quiet_bytes.size() == 1 and quiet_bytes[0] == 0 \
		and not receiver.is_attached

	var offset := Vector3(0.05, 0.1, -0.2)
	sender.attach_to_bone(SOCKET_BONE, SOCKET_SLOT)
	sender.set_local_offset(offset, Quaternion(Vector3(1.0, 0.0, 0.0), 0.5))
	var writer := CYGBitPacker.new()
	sender.serialize(writer)
	writer.write_uint32(FRAME_MARKER)
	var reader := CYGBitReader.new()
	reader.attach(writer.to_bytes())
	receiver.deserialize(reader)
	var applied: bool = receiver.is_attached and reader.read_uint32() == FRAME_MARKER \
		and not sender.has_delta()
	# A peer that only received the equip has no intent of its own to publish.
	var reecho := CYGBitPacker.new()
	receiver.serialize(reecho)
	var echo_bytes := reecho.to_bytes()
	var no_echo: bool = echo_bytes.size() == 1 and echo_bytes[0] == 0

	# Socket resolution is retried on the process tick, so let it run before judging
	# where the weapon ended up.
	await get_tree().create_timer(0.4).timeout
	var watcher_socket := watcher["socket"] as BoneAttachment3D
	var watcher_weapon := watcher["weapon"] as Node3D
	var seated: bool = watcher_weapon.get_parent() == watcher_socket \
		and watcher_weapon.position == offset
	var expected: Vector3 = watcher_socket.global_transform.origin \
		+ watcher_socket.global_transform.basis * offset
	var seated_globally: bool = watcher_weapon.global_position.distance_to(expected) < 0.001

	sender.detach()
	var off_writer := CYGBitPacker.new()
	sender.serialize(off_writer)
	var off_reader := CYGBitReader.new()
	off_reader.attach(off_writer.to_bytes())
	receiver.deserialize(off_reader)
	var unequipped: bool = not receiver.is_attached
	await get_tree().create_timer(0.4).timeout
	var unseated: bool = watcher_weapon.get_parent() != watcher_socket

	var passed: bool = quiet_ok and applied and no_echo and seated and seated_globally and unequipped \
		and unseated
	_check("bone_attachment_wire", passed)
	if not passed:
		print("[%s] attach diag: quiet=%s applied=%s no_echo=%s seated=%s global=%s unequipped=%s unseated=%s parent=%s" % [
			_role, quiet_ok, applied, no_echo, seated, seated_globally, unequipped, unseated,
			watcher_weapon.get_parent(),
		])
	carrier["avatar"].queue_free()
	carrier["weapon"].queue_free()
	watcher["avatar"].queue_free()
	watcher["weapon"].queue_free()



## Builds a `Node2D` + `CYGView` + `CYGTransformSync2D` rig owned by `owner`.
func _make_sync_rig(view_id: int, owner: int) -> Dictionary:
	var holder := Node2D.new()
	holder.name = "SyncRig%d" % view_id
	add_child(holder)
	var view := CYGView.new()
	view.name = "CYGView"
	view.view_id = view_id
	view.owner_actor = owner
	holder.add_child(view)
	var sync := CYGTransformSync2D.new()
	sync.name = "Sync"
	holder.add_child(sync)
	return {"node": holder, "view": view, "sync": sync}


## Loopback for the 2D sync: an authority rig and a proxy rig in one process,
## carried frame by frame through the view's own delta stream. Both rigs leave
## the network table first, so the tick pump cannot consume the deltas this test
## moves by hand.
var _sync2d_authority: Node2D = null
var _sync2d_steps := 0


func _step_sync2d(_tick: int) -> void:
	if _sync2d_authority == null or _sync2d_steps >= 24:
		return
	_sync2d_steps += 1
	var next := _sync2d_authority.position
	next.x += SYNC2D_STEP
	_sync2d_authority.position = next


func _run_transform_sync_2d() -> void:
	var local_actor: int = CYGNetwork.local_actor_number
	var authority := _make_sync_rig(SYNC2D_VIEW_A, local_actor)
	var proxy := _make_sync_rig(SYNC2D_VIEW_B, local_actor + 7777)
	CYGNetwork.unregister_view(SYNC2D_VIEW_A)
	CYGNetwork.unregister_view(SYNC2D_VIEW_B)

	var authority_node: Node2D = authority["node"]
	var authority_view: CYGView = authority["view"]
	var proxy_node: Node2D = proxy["node"]
	var proxy_view: CYGView = proxy["view"]
	_sync2d_authority = authority_node
	_sync2d_steps = 0
	CYGNetwork.network_tick.connect(_step_sync2d)

	var writer := CYGBitPacker.new()
	var reader := CYGBitReader.new()
	var samples: Array[float] = []
	var max_jump := 0.0
	var start_x := proxy_node.position.x
	var waited := 0.0
	while waited < 0.9:
		await get_tree().create_timer(1.0 / 60.0).timeout
		waited += 1.0 / 60.0
		writer.reset()
		authority_view.collect_deltas(writer)
		reader.attach(writer.to_bytes())
		proxy_view.apply_delta_stream(reader)

		var x := proxy_node.position.x
		if samples.is_empty():
			samples.append(x)
		elif not is_equal_approx(samples[samples.size() - 1], x):
			if samples.size() > 1:
				max_jump = maxf(max_jump, absf(x - samples[samples.size() - 1]))
			samples.append(x)

	CYGNetwork.network_tick.disconnect(_step_sync2d)
	# Give the interpolator one delay window to settle on the newest snapshot.
	await get_tree().create_timer(0.3).timeout
	_sync2d_authority = null

	var final_x := proxy_node.position.x
	var travelled: float = final_x - start_x
	# The proxy may overshoot the authority by up to one extrapolation window
	# (0.2s of the ramp speed) after the last snapshot, so the band is wide.
	var ok: bool = samples.size() >= 6 \
		and travelled >= 0.6 and travelled <= 2.4 \
		and max_jump <= SYNC2D_MAX_FRAME_JUMP \
		and absf(final_x - authority_node.position.x) <= 0.8
	_check("transform_sync_2d", ok)
	if not ok:
		print("[%s] 2d diag: samples=%d travelled=%.3f max_jump=%.3f final=%.3f authority=%.3f" % [
			_role, samples.size(), travelled, max_jump, final_x, authority_node.position.x,
		])
	authority_node.queue_free()
	proxy_node.queue_free()


## Minimal observed component: proves the dirty-mask walk in
## `CYGView.collect_deltas` / `apply_delta_stream` with a component that owns no
## sync properties, so the hand-driven path stays covered beside the avatar.
class ProbeSync:
	extends Node

	var pose := Vector3.ZERO
	var dirty := false

	func has_delta() -> bool:
		return dirty

	func serialize(writer: CYGBitPacker) -> void:
		writer.write_vector3(pose)

	func deserialize(reader: CYGBitReader) -> void:
		pose = reader.read_vector3()


## State the ledger rewinds for the deterministic synchronizer test: one position and
## one velocity, so a resimulated tick is hand-checkable arithmetic.
class RollbackDummy:
	extends RefCounted

	var position_x := 0.0
	var velocity := 0.0

	func save_state() -> Variant:
		return {"position_x": position_x, "velocity": velocity}

	func restore_state(state: Variant) -> void:
		if state is Dictionary:
			var authority_state: Dictionary = state
			position_x = authority_state.get("position_x", position_x)
			velocity = authority_state.get("velocity", velocity)

	## The ledger hands every input back exactly as `predict()` received it, so the
	## tick argument is only the number the replay is anchored to.
	func simulate(input: Variant = null, _value_tick: Variant = null) -> void:
		velocity = float(input)
		position_x += velocity


func _on_joined(room: String) -> void:
	if _joined:
		# A rejoin from the auto-reconnect below: the rigs already exist and the
		# seat's actor number may have moved, so only the recovery clock cares.
		_rejoins += 1
		print("[%s] rejoined '%s' as actor %d" % [
			_role, room, CYGNetwork.local_actor_number,
		])
		return
	_joined = true
	print("[%s] joined room '%s' as actor %d" % [
		_role, room, CYGNetwork.local_actor_number,
	])
	_check("join_room", room == ROOM and CYGNetwork.local_actor_number > 0)

	var holder := Node3D.new()
	add_child(holder)
	var view := CYGView.new()
	view.name = "CYGView"
	view.view_id = SCENE_VIEW_ID
	view.owner_actor = 1
	holder.add_child(view)
	view.bind_rpc("hello", _on_hello)
	CYGNetwork.register_view(view)

	_probe_sync = ProbeSync.new()
	_probe_sync.name = "ProbeSync"
	holder.add_child(_probe_sync)
	view.add_observable(_probe_sync)
	_view = view

	CYGNetwork.view_delta_received.connect(_on_view_delta)
	CYGNetwork.room_event.connect(_on_room_event)
	CYGNetwork.object_spawned.connect(_on_object_spawned)

	# The Godot packet pipe: a native-style `put_packet`/`get_packet` exchange
	# that rides the RPC space reserved for it (view 0, hash 0xFFFF).
	_peer = CYGMultiplayerPeer.new()
	_peer.create_client()
	_run_predictive_rollback()
	_run_transform_sync_2d()
	_check("authority_toggle_gate", _test_authority_toggle_gate())
	_run_dormancy_rig()
	_run_bone_attachment_rig()
	_build_audio_rig()
	_check_network_audio_dedup()

	if _role == "alice":
		# Counting transport first: every byte the rest of this run puts on the
		# wire, including the outage and the rejoin, reaches the profiler through it.
		_profiler_transport = CYGProfilingTransport.attach(CYGNetwork)
		# Bob's boot time is not deterministic, so the probe repeats until the run
		# ends instead of firing once and racing the second client's join.
		_run_sender()
		_run_avatar_authority()
		_run_rollback_authority()


## Emits one of every relayed packet shape.
func _send_probe() -> void:
	_view.send_rpc("hello", ["alice-greets", 42], CYGOpcodes.Target.ALL)
	_probe_sync.pose = PROBE_POSE
	_probe_sync.dirty = true
	var writer := CYGBitPacker.new()
	_view.collect_deltas(writer)
	_probe_sync.dirty = false
	CYGNetwork.send_view_delta(SCENE_VIEW_ID, writer.to_bytes())
	CYGNetwork.send_room_event("chat", "hello-from-alice".to_utf8_buffer())
	_peer.put_packet(NATIVE_PACKET_TEXT.to_utf8_buffer())
	# A one-shot event, fired every second so a late-joining peer is sure to hear one.
	if _audio_sync != null:
		_audio_sync.play_at(AUDIO_EVENT_AT, 1)


func _run_sender() -> void:
	while _elapsed < RUN_SECONDS:
		await get_tree().create_timer(1.0).timeout
		# The outage leaves the seat for a moment; sending into it would only prove
		# the loop can call a dead socket. Resuming on the rejoin is the point.
		if CYGNetwork.is_connected_flag:
			_send_probe()


## Spawns the avatar once both clients share the room, then walks it along +X on
## every network tick. `CYGNetwork.flush_view_deltas` ships the deltas; the
## harness sends nothing by hand, which is what a real game does.
func _run_avatar_authority() -> void:
	await get_tree().create_timer(2.0).timeout
	if not CYGNetwork.is_connected_flag:
		return
	_avatar_view = CYGNetwork.spawn_prefab(AVATAR_PREFAB_NAME, Vector3.ZERO)
	var actor: Node = _avatar_view.network_node if _avatar_view != null else null
	if not (actor is CYGActor3D):
		print("[alice] spawn diag: no CYGActor3D instance (actor=%s)" % actor)
		_check("actor_spawn", false)
		return
	var mine: bool = _avatar_view.is_mine
	var owned: bool = _avatar_view.owner_actor == CYGNetwork.local_actor_number
	var synced: bool = actor.get_node_or_null("TransformSync") is CYGTransformSync3D
	if not (mine and owned and synced):
		print("[alice] spawn diag: mine=%s owned=%s owner=%d local=%d sync=%s" % [
			mine, owned, _avatar_view.owner_actor,
			CYGNetwork.local_actor_number, actor.get_node_or_null("TransformSync"),
		])
	_check("actor_spawn", mine and owned and synced)
	_ownership_probe(actor as Node3D, true)
	CYGNetwork.network_tick.connect(_step_avatar)


func _step_avatar(_tick: int) -> void:
	if _avatar_view == null or not is_instance_valid(_avatar_view):
		return
	var actor := _avatar_view.network_node as Node3D
	if actor == null:
		return
	var next := actor.position
	next.x += AVATAR_STEP
	actor.position = next


func _on_object_spawned(view_id: int, _arguments: PackedByteArray) -> void:
	if _role != "bob":
		return
	var view := CYGNetwork.get_view(view_id)
	if view == null:
		return
	var actor := view.network_node as Node3D
	if actor == null:
		_check("actor_spawn", false)
		return
	# The rollback body is the second spawn and owns no `TransformSync`, so it claims
	# its own check instead of joining the avatar's interpolation sample.
	if actor.get_node_or_null("CharacterSync") != null:
		_setup_rollback_proxy(actor)
		return
	_proxy_view = view
	_check("actor_spawn", actor is CYGActor3D
		and not view.is_mine
		and view.owner_actor != CYGNetwork.local_actor_number
		and actor.get_node_or_null("TransformSync") is CYGTransformSync3D)
	if actor is CYGActor3D:
		var proxy_sync = actor.get_node_or_null("TransformSync")
		if proxy_sync == null or not is_instance_valid(proxy_sync):
			print("[bob] spawn diag: actor=%s view=%d owner=%d mine=%s sync=%s" % [
				actor, view.view_id, view.owner_actor, view.is_mine, proxy_sync,
			])
	_ownership_probe(actor, false)


## The toggle is purely local, so each side proves its own half: the owner keeps
## its input rig and hides the hitbox, the proxy does the opposite.
func _ownership_probe(actor: Node3D, is_owner: bool) -> void:
	var owner_only := actor.get_node_or_null("OwnerOnly") as Node
	var proxy_only := actor.get_node_or_null("ProxyOnly") as MeshInstance3D
	if owner_only == null or proxy_only == null:
		return
	if is_owner:
		_check("ownership_toggle", owner_only.is_processing() and not proxy_only.visible)
	else:
		_check("ownership_toggle", not owner_only.is_processing() and proxy_only.visible)


## Records the proxy's rendered pose per frame and judges it: many distinct positions
## along a monotonic ramp is what interpolation looks like, and a forward step bigger
## than one tick's travel anywhere in the run is a snap rather than a glide.
var _max_frame_back_step := 0.0


func _sample_proxy() -> void:
	if _checks.has("transform_interpolated"):
		return
	if _proxy_view == null or not is_instance_valid(_proxy_view):
		return
	var actor := _proxy_view.network_node as Node3D
	if actor == null:
		return
	var x := actor.position.x
	if _proxy_samples.is_empty():
		_proxy_samples.append(x)
		return
	var previous: float = _proxy_samples[_proxy_samples.size() - 1]
	if is_equal_approx(previous, x):
		return
	if _proxy_samples.size() > 1:
		var step: float = x - previous
		if step > 0.0:
			_max_frame_jump = maxf(_max_frame_jump, step)
		else:
			_max_frame_back_step = maxf(_max_frame_back_step, -step)
	_proxy_samples.append(x)

	if _max_frame_jump > AVATAR_MAX_FRAME_JUMP:
		_report_proxy_sample("snapped %.3f in one frame" % _max_frame_jump)
		_check("transform_interpolated", false)
		return
	var travelled: float = x - _proxy_samples[0]
	# A snapshot landing behind an extrapolated pose steps back a little, so the ramp is
	# only declared once a frame that moved forward has been seen: a transient reversal
	# must not be able to doom the whole run.
	if _proxy_samples.size() >= AVATAR_SAMPLES_MIN and travelled >= AVATAR_TRAVEL_MIN \
			and x > previous - 0.1:
		_check("transform_interpolated", true)


func _report_proxy_sample(reason: String) -> void:
	print("[bob] avatar diag: %s; samples=%d last=%.3f max_jump=%.3f max_back=%.3f" % [
		reason, _proxy_samples.size(),
		_proxy_samples[_proxy_samples.size() - 1] if not _proxy_samples.is_empty() else 0.0,
		_max_frame_jump, _max_frame_back_step,
	])


func _on_hello(args: Array) -> void:
	print("[%s] rpc hello args=%s" % [_role, str(args)])
	var ok: bool = args.size() == 2 and args[0] == "alice-greets" and args[1] == 42
	# The sender proves its own local echo; the relay is only proven by bob.
	_check("rpc_local_echo" if _role == "alice" else "rpc_received", ok)


func _on_view_delta(view_id: int, _payload: PackedByteArray) -> void:
	if view_id != SCENE_VIEW_ID or _role != "bob":
		return
	# The component itself is the assertion: the mask walk must reach it.
	_check("view_delta", _probe_sync.pose.is_equal_approx(PROBE_POSE))


func _on_room_event(event_name: String, body: PackedByteArray) -> void:
	if event_name != "chat" or _role != "bob":
		return
	_check("room_event", body.get_string_from_utf8() == "hello-from-alice")


func _process(delta: float) -> void:
	if _role == "parent":
		return
	_elapsed += delta
	if _role == "alice":
		_step_recovery()
	# Judged before the connection guard: an outage that never heals must still
	# end the run, or the parent waits on a client that has nothing left to do.
	# Bob outlives the sender so a late start cannot cost it the whole exchange.
	var limit := RUN_SECONDS if _role == "alice" else RUN_SECONDS * 1.6
	if _elapsed >= limit:
		_finish()
		return
	if not CYGNetwork.is_connected_flag:
		return
	if CYGNetwork.latency_ms > 0.0 and not _checks.has("ping_rtt"):
		print("[%s] ping RTT %.2f ms" % [_role, CYGNetwork.latency_ms])
		_check("ping_rtt", CYGNetwork.latency_ms < 50.0)
	while _peer != null and _peer.get_available_packet_count() > 0:
		var packet := _peer.get_packet()
		var sender := _peer.get_remote_sender_id()
		if _role == "bob":
			_check("godot_pipe", packet == NATIVE_PACKET_TEXT.to_utf8_buffer() and sender == 2)
	if _role == "bob":
		_sample_proxy()
		_sample_rollback_proxy()
		_sample_network_audio()


var _finished := false


## Phase 6 resilience, alice only: yank our own socket and hold the SDK to the
## promise that it recovers without help — `reconnecting` fired, and the room
## handed the seat back.
func _step_recovery() -> void:
	if _checks.has("reconnect_after_drop"):
		return
	if not _drop_triggered:
		if _elapsed < DROP_AT_SECONDS:
			return
		_drop_triggered = true
		print("[alice] dropping the socket at %.1fs" % _elapsed)
		CYGNetwork.client.simulate_drop()
		return
	if _elapsed < RECOVERY_VERDICT_SECONDS:
		return
	var recovered: bool = CYGNetwork.is_connected_flag and _reconnect_attempts > 0 and _rejoins > 0
	_check("reconnect_after_drop", recovered)
	if not recovered:
		print("[alice] recovery diag: connected=%s reconnects=%d rejoins=%d error=%s" % [
			CYGNetwork.is_connected_flag, _reconnect_attempts, _rejoins,
			CYGNetwork.last_error_message,
		])


## Phase 6 profiler: the wrapped transport counted this run's real traffic, so the
## ledger must be non-zero in both directions and the one-second ring must have
## committed at least one window.
func _report_profiler() -> void:
	var profiler := CYGNetworkProfiler.active
	if profiler == null:
		print("[alice] profiler diag: no active profiler; the transport never installed")
		_check("profiler_traffic", false)
		return
	var snapshot: Dictionary = profiler.snapshot()
	var totals: Dictionary = snapshot["totals"]
	var sent := int(totals["bytes_sent"])
	var received := int(totals["bytes_received"])
	var packets_out := int(totals["packets_sent"])
	var samples := int(snapshot["sample_count"])
	var deltas_out: Dictionary = snapshot["view_delta_bytes"]["out"]
	var passed: bool = sent > 0 and received > 0 and packets_out > 0 and samples > 0
	print("[alice] profiler: %d B sent / %d B received over %d packets, %d window(s), " % [
		sent, received, packets_out, samples,
	] + "view-delta keys=%s, rtt=%.2f ms" % [str(deltas_out.keys()), snapshot["rtt"]["avg_ms"]])
	_check("profiler_traffic", passed)
	if _profiler_transport != null:
		_profiler_transport.detach()
		_profiler_transport = null


func _finish() -> void:
	if _finished:
		return
	_finished = true
	if _role == "alice":
		_step_recovery()
		_report_profiler()
	if _role == "bob":
		_check("connected_flag", CYGNetwork.is_connected_flag)
		if not _checks.has("transform_interpolated"):
			_report_proxy_sample("never resolved into a ramp")
			_check("transform_interpolated", false)
		if not _checks.has("character_rollback"):
			print("[bob] rollback diag: never resolved; proxy=%s mispredict=%d max_speed=%.3f" % [
				_rollback_proxy, _rollback_max_mispredictions, _rollback_max_speed,
			])
			_check("character_rollback", false)
		if not _checks.has("network_audio_pooled"):
			print("[bob] audio diag: never heard; pool=%d slots_at_event=%d" % [
				_audio_pool_built, _audio_slots_at(AUDIO_EVENT_AT) if _audio_sync != null else -1,
			])
			_check("network_audio_pooled", false)
	_append_report("DONE")
	var network := get_node("/root/CYGNetwork")
	network.disconnect_from_room()
	get_tree().quit(0 if not _checks.values().has(false) else 1)


## Builds the events rig: one `CYGNetworkAudio3D` whose whole node budget is the
## pool it creates in `_ready`.
func _build_audio_rig() -> void:
	var holder := Node3D.new()
	holder.name = "AudioRig"
	add_child(holder)
	var view := CYGView.new()
	view.name = "CYGView"
	view.view_id = AUDIO_VIEW_ID
	view.owner_actor = CYGNetwork.local_actor_number
	holder.add_child(view)
	CYGNetwork.register_view(view)

	var clip := AudioStreamWAV.new()
	clip.format = AudioStreamWAV.FORMAT_16_BITS
	clip.mix_rate = 8000
	clip.stereo = false
	clip.data = PackedByteArray([0, 0, 0, 0])

	_audio_sync = CYGNetworkAudio3D.new()
	_audio_sync.name = "Footsteps"
	_audio_sync.pool_size = AUDIO_POOL_SIZE
	_audio_sync.stream = clip
	holder.add_child(_audio_sync)
	_audio_pool_built = _audio_sync.get_child_count()


## Number of pooled players sitting at `world_position`.
func _audio_slots_at(world_position: Vector3) -> int:
	var hits := 0
	for child in _audio_sync.get_children():
		var player := child as AudioStreamPlayer3D
		if player != null and player.global_position.is_equal_approx(world_position):
			hits += 1
	return hits


## The dedup ring is the only thing standing between a retransmit and the same
## footstep sounding twice, so a repeated identity must not reach a second slot.
func _check_network_audio_dedup() -> void:
	var identity := (CYGNetworkAudio3D.EVENT_ID_MASK << 24) | 0x5A
	_audio_sync._on_network_event([identity, AUDIO_FIRST_AT, 0.0, 1.0])
	var first := _audio_slots_at(AUDIO_FIRST_AT)
	_audio_sync._on_network_event([identity, AUDIO_REPEAT_AT, 0.0, 1.0])
	var repeated := _audio_slots_at(AUDIO_REPEAT_AT)
	var grew := _audio_sync.get_child_count() - _audio_pool_built
	var passed: bool = first == 1 and repeated == 0 and grew == 0
	_check("network_audio_dedup", passed)
	if not passed:
		print("[%s] dedup diag: first=%d repeat=%d pool_before=%d pool_now=%d" % [
			_role, first, repeated, _audio_pool_built, _audio_sync.get_child_count(),
		])


## Called from `_process` while bob waits for alice's event: hearing it means one of
## the players that already existed moved to the position the sender named, and the
## rig gained no node.
func _sample_network_audio() -> void:
	if _audio_sync == null or _checks.has("network_audio_pooled"):
		return
	if _audio_sync.get_child_count() != _audio_pool_built:
		_check("network_audio_pooled", false)
		return
	if _audio_slots_at(AUDIO_EVENT_AT) >= 1:
		_check("network_audio_pooled", true)


func _check(name: String, passed: bool) -> void:
	# First result wins so a later duplicate cannot mask a failure.
	if _checks.has(name):
		return
	_checks[name] = passed
	var line := "CHECK %s %s" % [name, "PASS" if passed else "FAIL"]
	print(line)
	if _role != "parent":
		_append_report(line)


func _test_bitpacker() -> bool:
	var writer := CYGBitPacker.new()
	writer.write_int32(-123456)
	writer.write_uint32(300000)
	writer.write_int64(-9000000000)
	writer.write_string("héllo")
	writer.write_vector3(Vector3(1.5, -2.25, 3.125))
	writer.write_quaternion(Quaternion(0.1, 0.2, 0.3, 0.9).normalized())
	writer.write_bool(true)

	var reader := CYGBitReader.new()
	reader.attach(writer.to_bytes())
	var ok := true
	ok = ok and reader.read_int32() == -123456
	ok = ok and reader.read_uint32() == 300000
	ok = ok and reader.read_int64() == -9000000000
	ok = ok and reader.read_string() == "héllo"
	ok = ok and reader.read_vector3().is_equal_approx(Vector3(1.5, -2.25, 3.125))
	var quaternion := reader.read_quaternion()
	ok = ok and absf(quaternion.length() - 1.0) < 0.01
	ok = ok and reader.read_bool()
	return ok
