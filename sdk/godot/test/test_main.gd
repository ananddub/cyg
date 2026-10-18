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
const RUN_SECONDS := 9.0
const MAX_ATTEMPTS := 4
## Every check the two clients produce together; the parent fails without any one.
const REQUIRED_CHECKS := [
	"bitpacker_roundtrip", "join_room", "ping_rtt", "connected_flag",
	"rpc_local_echo", "rpc_received", "view_delta", "room_event", "godot_pipe",
]

var _role := "parent"
var _checks := {}
var _elapsed := 0.0
var _joined := false
var _peer: CYGMultiplayerPeer = null
var _view: CYGView = null
var _probe_sync: ProbeSync = null


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

	var network := get_node("/root/CYGNetwork")
	var settings := CYGServerSettings.new()
	settings.server_http_url = SERVER_URL
	network.settings = settings
	network.joined_room.connect(_on_joined)
	network.connect_to_room(ROOM, _role)


## Minimal observed component: proves the dirty-mask walk in
## `CYGView.collect_deltas` / `apply_delta_stream` without a Phase 2 sync script.
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

	# The Godot packet pipe: a native-style `put_packet`/`get_packet` exchange
	# that rides the RPC space reserved for it (view 0, hash 0xFFFF).
	_peer = CYGMultiplayerPeer.new()
	_peer.create_client()

	if _role == "alice":
		# Bob's boot time is not deterministic, so the probe repeats until the run
		# ends instead of firing once and racing the second client's join.
		_run_sender()


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
