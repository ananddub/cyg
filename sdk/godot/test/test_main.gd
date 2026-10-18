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
const MAX_ATTEMPTS := 5
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
## Every check the two clients produce together; the parent fails without any one.
const REQUIRED_CHECKS := [
	"bitpacker_roundtrip", "join_room", "ping_rtt", "connected_flag",
	"rpc_local_echo", "rpc_received", "view_delta", "room_event", "godot_pipe",
	"sync_properties", "actor_spawn", "transform_interpolated",
	"transform_sync_2d", "ownership_toggle",
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


func _ready() -> void:
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("role="):
			_role = argument.trim_prefix("role=")

	if _role == "parent":
		_run_parent()
		return
	_run_child()


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
	_install_avatar_prefab()

	var network := get_node("/root/CYGNetwork")
	var settings := CYGServerSettings.new()
	settings.server_http_url = SERVER_URL
	network.settings = settings
	network.joined_room.connect(_on_joined)
	network.connect_to_room(ROOM, _role)


## Registers the networked avatar prefab from `test/avatar_prefab.tscn`, the
## editor-authored shape a game ships: `CYGActor3D` root + `CYGView` +
## `CYGTransformSync3D` + `CYGOwnershipToggle` with its two node groups.
func _install_avatar_prefab() -> void:
	var packed := load(AVATAR_PREFAB_SCENE) as PackedScene
	if packed == null:
		push_error("harness: %s did not load as a PackedScene." % AVATAR_PREFAB_SCENE)
		return

	var entry := CYGPrefabEntry.new()
	entry.id = AVATAR_PREFAB_ID
	entry.name = AVATAR_PREFAB_NAME
	entry.scene = packed
	var registry := CYGPrefabRegistry.new()
	registry.entries = [entry]
	CYGNetwork.prefabs = registry


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


func _on_joined(room: String) -> void:
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
	_run_transform_sync_2d()

	if _role == "alice":
		# Bob's boot time is not deterministic, so the probe repeats until the run
		# ends instead of firing once and racing the second client's join.
		_run_sender()
		_run_avatar_authority()


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


func _run_sender() -> void:
	while _elapsed < RUN_SECONDS and CYGNetwork.is_connected_flag:
		await get_tree().create_timer(1.0).timeout
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
	_proxy_view = view
	var actor := view.network_node as Node3D
	if actor == null:
		_check("actor_spawn", false)
		return
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


## Records the proxy's rendered pose per frame: many distinct positions along a
## monotonic ramp with no large step is what interpolation looks like.
func _sample_proxy() -> void:
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
		_max_frame_jump = maxf(_max_frame_jump, absf(x - previous))
	_proxy_samples.append(x)

	var travelled: float = x - _proxy_samples[0]
	if _proxy_samples.size() >= AVATAR_SAMPLES_MIN and travelled >= AVATAR_TRAVEL_MIN:
		# A new snapshot landing behind an extrapolated pose may step back a
		# little; anything larger than that is a jump, not interpolation.
		_check("transform_interpolated", x > previous - 0.1
			and _max_frame_jump <= AVATAR_MAX_FRAME_JUMP)


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
	# Bob outlives the sender so a late start cannot cost it the whole exchange.
	var limit := RUN_SECONDS if _role == "alice" else RUN_SECONDS * 1.6
	if _elapsed >= limit:
		_finish()


var _finished := false


func _finish() -> void:
	if _finished:
		return
	_finished = true
	if _role == "bob":
		_check("connected_flag", CYGNetwork.is_connected_flag)
	_append_report("DONE")
	var network := get_node("/root/CYGNetwork")
	network.disconnect_from_room()
	get_tree().quit(0 if not _checks.values().has(false) else 1)


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
