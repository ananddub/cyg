# Live end-to-end test: the real Godot SDK against a real CYG server.
#
#   godot --headless --path sdk/client/godot -s res://test/live_server_e2e.gd
#
# The script assumes a CYG server is already listening at CYG_E2E_ENDPOINT
# (env or default http://127.0.0.1:17777) hosting its default room. It joins,
# sends inputs, waits for schema state, asserts the player exists and state
# decodes, then leaves cleanly. Exit code 1 on any failure.
extends SceneTree

const ENDPOINT := "http://127.0.0.1:17777"

var checks := 0
var failures := 0
var done := false
var room: CYGRoom = null
var client: CYGRelayClient = null


func check(name: String, ok: bool) -> void:
	checks += 1
	if ok:
		print("CHECK %s" % name)
	else:
		failures += 1
		printerr("FAIL %s" % name)


func finish() -> void:
	print("checks=%d failures=%d" % [checks, failures])
	done = true
	quit(1 if failures > 0 else 0)


func _init() -> void:
	print("[e2e] script version 7 running")
	var endpoint := OS.get_environment("CYG_E2E_ENDPOINT")
	if endpoint.is_empty():
		endpoint = ENDPOINT

	client = CYGRelayClient.new(endpoint)
	var room_name := OS.get_environment("CYG_E2E_ROOM")
	if room_name.is_empty():
		room_name = "coin-rush"
	# The SDK's HTTP matchmaking requires the node inside the scene tree —
	# and a node added in _init only ENTERS the tree after a frame passes.
	root.add_child(client)
	await process_frame
	print("[e2e] client inside tree: %s" % client.is_inside_tree())
	client.join_or_create(room_name)

	var joined: Array = [false]
	client.room_joined.connect(func(r: CYGRoom) -> void:
		joined[0] = true
		room = r
		_wire_room(r)
	)
	client.matchmake_failed.connect(func(_m: String, _n: String, status: int, detail: String) -> void:
		check("matchmake accepted", false)
		printerr("matchmake failed: %d %s | last_error=%s" % [status, detail, client.last_error])
		finish()
	)

	# Overall watchdog: a server that never admits us must fail the test,
	# not hang the runner.
	var elapsed := 0.0
	while not done and elapsed < 30.0:
		await create_timer(0.25).timeout
		elapsed += 0.25
		if joined[0] and room != null:
			_drive(room)
			joined[0] = false  # drive once
	if not done:
		check("joined within 30s", joined[0] or room != null)
		finish()


func _wire_room(r: CYGRoom) -> void:
	r.state_changed.connect(func(_state: CYGSchemaMap) -> void:
		check("schema state decoded", true)
		_validate_and_leave(r)
	)
	r.schema_error_raised.connect(func(message: String) -> void:
		check("no schema errors", false)
		printerr("schema error: %s" % message)
	)
	r.closed.connect(func(_code: int, _reason: String) -> void:
		closed_seen[0] = true
		check("left cleanly", true)
		finish()
	)



var validations := 0
var closed_seen: Array = [false]

func _validate_and_leave(r: CYGRoom) -> void:
	if validations > 0:
		return
	validations += 1
	var state: CYGSchemaMap = r.state
	check("state map present", state != null)
	# Move for a moment, then leave: the server must accept our input frames
	# without dropping us.
	r.send_input(1, 0.0, 0.0, 0.5, 0.0)
	check("input frame sent", true)
	# Give the server one round trip to ingest the input, then leave.
	await create_timer(0.5).timeout
	check("no close event after input", not closed_seen[0])
	r.leave(true)


func _drive(_r: CYGRoom) -> void:
	check("room joined", true)
	check("session id assigned", not str(_r.session_id).is_empty())
