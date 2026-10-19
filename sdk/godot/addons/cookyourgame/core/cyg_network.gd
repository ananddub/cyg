extends Node
## Global facade of the CookYourGame client SDK (autoload `CYGNetwork`).
##
## Owns the transport, the room roster, the view table and the spawn pump.
## Everything a game touches lives here or on `CYGView`: connect, spawn,
## RPC and state-sync calls all funnel through this singleton.

signal connected_to_server
signal joined_room(room_name: String)
signal disconnected(reason: String)
## A dropped socket is being retried: which attempt this is, and how long the SDK waits
## before it fires. Emitted once per scheduled retry, never for a consented leave.
signal reconnecting(attempt: int, delay_seconds: float)
signal room_error(code: int, message: String)
signal room_event(event_name: String, body: PackedByteArray)
signal room_state_received(payload: PackedByteArray)
signal room_data_bytes(payload: PackedByteArray)
signal view_delta_received(view_id: int, payload: PackedByteArray)
signal object_spawned(view_id: int, arguments: PackedByteArray)
signal object_despawned(view_id: int)
signal player_joined(player: CYGPlayer)
signal player_left(player: CYGPlayer)
signal rewind_hit_received(payload: PackedByteArray)
## Fixed-rate simulation tick that observed components sample their deltas on.
signal network_tick(tick: int)
## Raw `MultiplayerPeer` packet from the reserved RPC pipe; the first two bytes
## are the sender's actor number.
signal godot_packet_received(packet: PackedByteArray)

const PING_INTERVAL_SECONDS := 5.0
const EVENT_PLAYER_JOIN := "cyg_player_join"
const EVENT_PLAYER_LEAVE := "cyg_player_leave"
const MAX_EVENT_NAME_BYTES := 96
const INVALID_PREFAB_ID := 0xFFFF
## First reconnect wait, in seconds; each later attempt doubles it. Three attempts
## (`CYGSessionCookie.MAX_RECONNECT_ATTEMPTS`) therefore cover 3.5 seconds of outage
## before the SDK stops, which is a handover, not a server outage.
const RECONNECT_BASE_DELAY_SECONDS := 0.5
## How long a retry that produced neither a socket nor a refusal is given before the
## next attempt is spent on it: a seat reservation that answers nothing is answered for.
const RECONNECT_ATTEMPT_TIMEOUT_SECONDS := 3.0

## The retry's own state machine, so a drop is retried on a growing delay without a Timer
## node and without racing the transport's signals.
enum ReconnectPhase { IDLE, WAITING, TRYING }

var client: CYGClient
var settings: CYGServerSettings = CYGServerSettings.new()
var prefabs: CYGPrefabRegistry = null
## Container that networked instances spawn into; falls back to the current scene.
var spawn_root: Node = null
var local_player: CYGPlayer = null
var players: Dictionary = {}  # actor -> CYGPlayer
var latency_ms: float = 0.0
var last_error_code: int = 0
var last_error_message: String = ""
var room_name: String = ""
## Network ticks per second; the cadence authority peers sample deltas at.
var tick_rate: int = 30
var current_tick: int = 0
## Retries the room on the SDK's own when the socket dies without being asked to. The
## retry replays the server-issued reconnection token while the seat still holds one and
## otherwise makes a fresh join of the same room name, so a laptop that slept through a
## wifi handover comes back to the room it left. Set it false to handle `disconnected`
## yourself.
var auto_reconnect: bool = true
## This install's persistent identity: it records that a drop is worth retrying and how
## many retries have already been spent, so a restart does not buy a fresh budget.
var session_cookie: CYGSessionCookie = CYGSessionCookie.new()

## Room-wide rollback clock. Predicting components read their ticks from here
## instead of creating a private one, so every rewind in the room lands on the
## same number as the deltas `flush_view_deltas()` ships.
var network_time: CYGNetworkTime = CYGNetworkTime.new()

var _view_table: Dictionary = {}  # view_id -> CYGView
var _view_index := 0
var _last_ping_at := 0.0
var _last_ping_sent_at := 0.0
var _join_handshake_complete := false
var _pending_nickname := ""
var _tick_accumulator := 0.0
var _sent_in_window := 0
var _received_in_window := 0
var _window_accum := 0.0
var _reconnect_phase := ReconnectPhase.IDLE
var _reconnect_deadline := 0.0
var _intentional_leave := false
## Whether the next retry may replay the seat token. The first refusal retires it: a
## reaped seat never comes back, and every further token replay is a round trip to the
## same error while a join by room name would have put the player back in.
var _reconnect_with_token := true

## Deltas this peer sent and received during the last completed second. A rate rather
## than a running total, because the question a profiler answers is "what is this tick
## costing", and a total only ever goes up.
var deltas_sent_per_second: int = 0
var deltas_received_per_second: int = 0

## Views this peer replicates, whether it owns them or only receives them.
var view_count: int:
	get:
		return _view_table.size()


func _ready() -> void:
	if client == null:
		client = CYGClient.new()
	client.host = self
	client.settings = settings
	client.on_ping_sent = _record_ping_sent
	client.frame_received.connect(_handle_frame)
	client.socket_opened.connect(_handle_socket_open)
	client.socket_closed.connect(_handle_socket_closed)
	client.server_error.connect(_handle_server_error)
	client.seat_failed.connect(_handle_seat_failed)
	session_cookie.load_or_create()
	process_priority = 1000


func _record_ping_sent() -> void:
	_last_ping_sent_at = Time.get_ticks_msec() / 1000.0


func _process(delta: float) -> void:
	client.poll()
	_run_reconnect_clock()
	_advance_rate_window(delta)
	network_time.ticks_per_second = tick_rate
	network_time.latency_ms = latency_ms
	network_time.advance(delta * 1000.0)
	var interval := 1.0 / float(maxi(tick_rate, 1))
	_tick_accumulator += delta
	# Catch up over at most one tick per frame: a stalled frame must not spend
	# the backlog replaying history that is already obsolete.
	if _tick_accumulator >= interval:
		_tick_accumulator = fmod(_tick_accumulator, interval)
		current_tick += 1
		# The clock re-anchors to the counter every peer is sampling against, so a
		# late frame shifts the wall-time expectation rather than the tick numbering.
		network_time.observe_tick(current_tick)
		network_tick.emit(current_tick)
		flush_view_deltas()


var is_connected_flag: bool:
	get:
		return client != null and client.is_connected


var local_actor_number: int:
	get:
		return local_player.actor_number if local_player != null else 0


var is_master_client: bool:
	get:
		return local_player != null and local_player.is_master_client


## ---------------------------------------------------------------- connection

func join_or_create_room(room: String, player_name: String = "", max_players: int = 0) -> void:
	if max_players > 0:
		settings.max_players = max_players
	_connect_to_room(room, player_name)


func connect_to_room(room: String, player_name: String = "") -> void:
	_connect_to_room(room, player_name)


func _connect_to_room(room: String, player_name: String) -> void:
	_pending_nickname = player_name
	room_name = CYGServerSettings.normalize_room_name(room)
	client.settings = settings
	client.connect_to_room(room, player_name)


## Opens the socket directly on a room url, skipping seat reservation.
func connect_url(url: String, player_name: String = "") -> void:
	_pending_nickname = player_name
	client.connect_socket(url, "")


func disconnect_from_room() -> void:
	_intentional_leave = true
	_reconnect_phase = ReconnectPhase.IDLE
	client.disconnect_from_room()
	_clear_room_state()


## Resumes a dropped seat using the reconnection token of the last join.
func reconnect_room() -> void:
	var token := client.reconnection_token()
	if token.is_empty():
		push_warning("CYGNetwork: no reconnection token; cannot resume the seat.")
		return
	client.reconnect_to_room(room_name, token)


## ------------------------------------------------------------------ sending

## Frames a payload as a canonical packet and hands it to the transport.
func send_frame(op: int, payload: PackedByteArray) -> bool:
	return client.send_frame(op, payload)


## Sends a pre-encoded RPC payload: [viewId u32 BE][hash u16 BE][target u8][args].
func send_rpc_payload(payload: PackedByteArray) -> bool:
	return client.send_frame(CYGOpcodes.CustomRpc, payload)


## Sends a view sync delta as [zigzag varint viewId][payload].
func send_view_delta(view_id: int, delta_payload: PackedByteArray) -> bool:
	var writer := CYGBitPacker.new()
	writer.write_int32(view_id)
	writer.write_raw(delta_payload)
	return client.send_frame(CYGOpcodes.StateDelta, writer.to_bytes())


## Sends a player input payload, opaque to the SDK.
func send_input_payload(payload: PackedByteArray) -> bool:
	return client.send_frame(CYGOpcodes.PlayerInput, payload)


## Pushes every view with pending observed-component state onto the wire, one
## packet per view, at the tick boundary.
##
## Only authority peers mark components dirty (`CYGBehaviour.set_property` and
## the sync scripts gate on `has_authority`), so a proxy never has anything to
## flush and needs no ownership filter here.
func flush_view_deltas() -> void:
	if not is_connected_flag:
		return
	for entry in _view_table.values():
		var view := entry as CYGView
		if view == null or view.network_dormant or not view.has_any_delta():
			continue
		var writer := CYGBitPacker.new()
		view.collect_deltas(writer)
		if send_view_delta(view.view_id, writer.to_bytes()):
			_sent_in_window += 1


## Rolls the one-second measurement window and publishes its counts.
func _advance_rate_window(delta: float) -> void:
	_window_accum += delta
	if _window_accum < 1.0:
		return
	_window_accum = fmod(_window_accum, 1.0)
	deltas_sent_per_second = _sent_in_window
	deltas_received_per_second = _received_in_window
	_sent_in_window = 0
	_received_in_window = 0


## Sends a named room event as [LEB128 name length][utf8 name][body].
##
## It travels tunneled under ROOM_DATA_BYTES rather than as bare ROOM_DATA (13):
## a relay room hands 13 to its guest script and drops it when there is none, so
## an event would never reach the other peers.
func send_room_event(event_name: String, body: PackedByteArray = PackedByteArray()) -> bool:
	if event_name.is_empty():
		push_error("CYGNetwork.send_room_event requires an event name.")
		return false
	var writer := CYGBitPacker.new()
	writer.write_string(event_name)
	writer.write_raw(body)
	return client.send_frame(CYGOpcodes.RoomDataBytes, writer.to_bytes())


## Requests a room-script call and awaits its ROOM_RESPONSE (Phase 6 surface).
func send_room_request(payload: PackedByteArray) -> bool:
	return client.send_wire(CYGOpcodes.WireRoomRequest, payload)


## ------------------------------------------------------------- object spawn

## Instantiates a registered prefab locally and tells the room about it.
func spawn_prefab(prefab_name: String, position: Vector3, rotation: Quaternion = Quaternion.IDENTITY) -> CYGView:
	var prefab_id := prefabs.get_prefab_id(prefab_name) if prefabs != null else INVALID_PREFAB_ID
	if prefab_id == INVALID_PREFAB_ID:
		push_error("CYGNetwork.spawn_prefab: unknown prefab '%s'." % prefab_name)
		return null
	var scene := prefabs.get_prefab(prefab_id)
	if scene == null:
		return null

	var owner_actor := local_actor_number
	var view_id := next_view_id_for_owner(owner_actor)
	var view := _acquire_instance(scene, position, rotation)
	view.assign_view_id(view_id, owner_actor)

	var writer := CYGBitPacker.new()
	# The relay registry keys entities and spawn owners by the zigzag varint the
	# Unity SDK writes here, so an owner-targeted RPC can resolve this view id.
	writer.write_int32(view_id)
	writer.write_uint16_be(prefab_id)
	writer.write_uint32(owner_actor)
	writer.write_vector3(position)
	writer.write_quaternion_raw(rotation)
	send_frame(CYGOpcodes.SpawnObject, writer.to_bytes())

	view.invoke_spawn_hooks()
	return view


## Tears a view down locally and tells the room about it.
func destroy_object(view: CYGView) -> void:
	if view == null:
		return
	if view.view_id != 0:
		var writer := CYGBitPacker.new()
		writer.write_int32(view.view_id)
		send_frame(CYGOpcodes.DespawnObject, writer.to_bytes())
	_release_instance(view)


## Allocates a view id owned by an actor: (actor << 16) | counter.
func next_view_id_for_owner(actor_number: int) -> int:
	if actor_number < 0:
		actor_number = 0
	_view_index = (_view_index + 1) & 0xFFFF
	return (actor_number << 16) | _view_index


## ------------------------------------------------------------- view lookups

func register_view(view: CYGView) -> void:
	if view.view_id > 0:
		_view_table[view.view_id] = view


func unregister_view(view_id: int) -> void:
	if view_id > 0:
		_view_table.erase(view_id)
		CYGRpcRouter.unbind_id(view_id)


func get_view(view_id: int) -> CYGView:
	return _view_table.get(view_id)


func all_views() -> Array:
	return _view_table.values()


## ------------------------------------------------------------------- routing

func _handle_frame(frame: PackedByteArray) -> void:
	var payload_length := (int(frame[0]) << 8) | int(frame[1])
	if CYGOpcodes.HEADER_SIZE + payload_length > frame.size():
		push_warning("CYGNetwork: frame header claims %d payload bytes but only %d arrived."
			% [payload_length, frame.size() - CYGOpcodes.HEADER_SIZE])
		return
	var op := frame[2]
	var payload := frame.slice(CYGOpcodes.HEADER_SIZE, CYGOpcodes.HEADER_SIZE + payload_length)
	_route(op, payload)


func _route(op: int, payload: PackedByteArray) -> void:
	match op:
		CYGOpcodes.Ping, CYGOpcodes.Pong:
			# The server echoes our ping verbatim; either direction measures RTT.
			latency_ms = (Time.get_ticks_msec() / 1000.0 - _last_ping_sent_at) * 1000.0
		CYGOpcodes.WireJoinRoom:
			_handle_join_handshake(payload)
		CYGOpcodes.WireRoomState, CYGOpcodes.StateSnapshot:
			room_state_received.emit(payload)
		CYGOpcodes.WireRoomData:
			_handle_room_data(payload)
		CYGOpcodes.StateDelta:
			_handle_view_delta(payload)
		CYGOpcodes.CustomRpc:
			CYGRpcRouter.handle_received(payload)
		CYGOpcodes.SpawnObject:
			_handle_spawn(payload)
		CYGOpcodes.DespawnObject:
			_handle_despawn(payload)
		CYGOpcodes.RewindHit:
			rewind_hit_received.emit(payload)
		CYGOpcodes.RoomDataBytes:
			_handle_room_bytes(payload)
		CYGOpcodes.WireRoomResponse:
			room_event.emit("cyg_room_response", payload)
		_:
			push_warning("CYGNetwork: no route for opcode %d." % op)


func _handle_join_handshake(payload: PackedByteArray) -> void:
	var reader := CYGBitReader.new()
	reader.attach(payload)
	var reconnection_token := ""
	if reader.remaining > 0:
		var token_length := reader.read_byte()
		if token_length > 0 and reader.remaining >= token_length:
			reconnection_token = reader.read_bytes(token_length).get_string_from_utf8()
	var serializer := ""
	if reader.remaining > 0:
		var serializer_length := reader.read_byte()
		if serializer_length > 0 and reader.remaining >= serializer_length:
			serializer = reader.read_bytes(serializer_length).get_string_from_utf8()

	if room_name.is_empty():
		# Direct `connect_url` joins have no requested name; the seat token the
		# server echoes back is "<raw request path>:<sessionId>", so its first
		# segment names the room.
		room_name = reconnection_token.split("?")[0].split(":")[-1] if not reconnection_token.is_empty() else CYGServerSettings.DEFAULT_ROOM_NAME

	if not serializer.is_empty() and serializer != CYGOpcodes.SERIALIZER_ID:
		push_warning("CYGNetwork: room serializer \"%s\" is not the schema codec this SDK decodes." % serializer)

	_register_local_player()
	# A seat is held again: the retry budget is for this outage, not for the process.
	session_cookie.resolve_reconnect()
	_reconnect_phase = ReconnectPhase.IDLE
	_reconnect_with_token = true
	_join_handshake_complete = true
	client.ack_join()
	joined_room.emit(room_name)


func _handle_room_data(payload: PackedByteArray) -> void:
	var split := _split_event_name(payload)
	var event_name: String = split["name"]
	var body: PackedByteArray = split["body"]
	_track_roster_event(event_name, body)
	room_event.emit(event_name, body)


func _handle_room_bytes(payload: PackedByteArray) -> void:
	var split := _split_event_name(payload)
	var event_name: String = split["name"]
	if not event_name.is_empty():
		_track_roster_event(event_name, split["body"])
		room_event.emit(event_name, split["body"])
		return
	room_data_bytes.emit(payload)


func _handle_view_delta(payload: PackedByteArray) -> void:
	_received_in_window += 1
	var reader := CYGBitReader.new()
	reader.attach(payload)
	var view_id := reader.read_int32()
	var body := payload.slice(reader.bytes_read)
	var view := get_view(view_id)
	if view != null:
		var delta_reader := CYGBitReader.new()
		delta_reader.attach(body)
		view.apply_delta_stream(delta_reader)
	view_delta_received.emit(view_id, body)


func _handle_spawn(payload: PackedByteArray) -> void:
	var reader := CYGBitReader.new()
	reader.attach(payload)
	var view_id := int(reader.read_int32())
	var prefab_id := reader.read_uint16_be()
	var owner_actor := int(reader.read_uint32())
	var position := reader.read_vector3()
	var rotation := reader.read_quaternion_raw()
	var arguments := payload.slice(reader.bytes_read)

	var scene := prefabs.get_prefab(prefab_id) if prefabs != null else null
	if scene == null:
		push_error("CYGNetwork: remote spawn references unknown prefab id %d." % prefab_id)
		return

	var view := _acquire_instance(scene, position, rotation)
	view.assign_view_id(view_id, owner_actor)
	view.invoke_spawn_hooks()
	object_spawned.emit(view_id, arguments)


func _handle_despawn(payload: PackedByteArray) -> void:
	var reader := CYGBitReader.new()
	reader.attach(payload)
	var view_id := int(reader.read_int32())
	var view := get_view(view_id)
	if view != null:
		_release_instance(view)
	object_despawned.emit(view_id)


## --------------------------------------------------------------- membership

func _register_local_player() -> void:
	local_player = CYGPlayer.new()
	local_player.player_id = client.session_id()
	local_player.nickname = _pending_nickname if not _pending_nickname.is_empty() else "Player"
	local_player.is_local = true
	local_player.is_master_client = true
	local_player.actor_number = _derive_actor_number(local_player.player_id)
	local_player.custom_properties["room"] = room_name
	players.clear()
	players[local_player.actor_number] = local_player


func _track_roster_event(event_name: String, body: PackedByteArray) -> void:
	if event_name.is_empty() or local_player == null:
		return
	if event_name == EVENT_PLAYER_JOIN:
		var reader := CYGBitReader.new()
		reader.attach(body)
		var actor := int(reader.read_uint32())
		var nickname := reader.read_string() if reader.remaining > 0 else ""
		if actor <= 0 or actor == local_player.actor_number:
			return
		var remote := CYGPlayer.new(actor, nickname)
		remote.player_id = "actor-%d" % actor
		players[actor] = remote
		_reevaluate_master_client()
		player_joined.emit(remote)
	elif event_name == EVENT_PLAYER_LEAVE:
		var reader := CYGBitReader.new()
		reader.attach(body)
		var actor := int(reader.read_uint32())
		var leaving: CYGPlayer = players.get(actor)
		if leaving != null and players.erase(actor):
			_reevaluate_master_client()
			if actor != local_player.actor_number:
				player_left.emit(leaving)


func _reevaluate_master_client() -> void:
	var lowest := 0x7FFFFFFF
	for actor in players.keys():
		lowest = mini(lowest, actor)
	for actor in players.keys():
		(players[actor] as CYGPlayer).is_master_client = actor == lowest


## ------------------------------------------------------------------ plumbing

func _handle_socket_open() -> void:
	_join_handshake_complete = false
	last_error_code = 0
	last_error_message = ""
	_last_ping_at = Time.get_ticks_msec() / 1000.0
	_last_ping_sent_at = _last_ping_at
	# The server initiates JOIN_ROOM on this protocol; we answer it in
	# `_handle_join_handshake`, so nothing is sent here.
	connected_to_server.emit()


func _handle_socket_closed(reason: String, code: int) -> void:
	_clear_room_state()
	disconnected.emit(reason)
	var consented := _intentional_leave
	_intentional_leave = false
	if consented or not auto_reconnect or room_name.is_empty():
		return
	# The relay answers any closure with its own consented-leave close code, so the wire
	# code cannot tell "the player left" from "the link died": `_intentional_leave` above
	# is the only honest signal, and it already had its veto. A retry-inviting 4010 is
	# passed through as the hint it is; every other code must not talk the cookie out of
	# a reconnect the local side never asked for.
	session_cookie.note_disconnect(reason,
		code if code == CYGOpcodes.CloseMayTryReconnect else 0)
	_schedule_reconnect()


## Queues the retry, or says out loud that there is nothing left to try. A room that is
## genuinely gone is refused faster than a handover that takes a second, and a game that
## would rather ask the player gets the `room_error` before it burns another join.
func _schedule_reconnect() -> void:
	if not session_cookie.should_attempt_reconnect():
		_give_up_reconnect()
		return
	var attempt := session_cookie.reconnect_attempts + 1
	var delay := RECONNECT_BASE_DELAY_SECONDS * pow(2.0, float(attempt - 1))
	_reconnect_deadline = _clock_seconds() + delay
	_reconnect_phase = ReconnectPhase.WAITING
	reconnecting.emit(attempt, delay)


## Drives the retry from the frame pump: wait, fire, and if nothing came back, wait
## again. Wall-clock deadlines, so a stalled frame costs a retry rather than postponing
## one, and a game that reconnects by hand is noticed before the timer fires.
func _run_reconnect_clock() -> void:
	match _reconnect_phase:
		ReconnectPhase.WAITING:
			if _clock_seconds() < _reconnect_deadline:
				return
			if is_connected_flag:
				_reconnect_phase = ReconnectPhase.IDLE
				return
			if not session_cookie.begin_reconnect_attempt():
				_give_up_reconnect()
				return
			_reconnect_phase = ReconnectPhase.TRYING
			_reconnect_deadline = _clock_seconds() + RECONNECT_ATTEMPT_TIMEOUT_SECONDS
			_retry_seat()
		ReconnectPhase.TRYING:
			if is_connected_flag:
				return
			if _clock_seconds() >= _reconnect_deadline:
				_schedule_reconnect()
		_:
			pass


## Re-seats this peer: the server-issued token while the seat is still held, a fresh join
## by room name once the server has reaped it. The second costs an actor number and a
## full state rebuild, and is the honest price of an outage longer than the grace period.
func _retry_seat() -> void:
	var token := client.reconnection_token()
	if token.is_empty() or not _reconnect_with_token:
		_connect_to_room(room_name, _pending_nickname)
		return
	client.reconnect_to_room(room_name, token)


## One failed retry is enough to learn the seat is gone: retire the token and let the
## attempt already in flight time out into a join by room name, which is the only way
## back into a room that has reaped the seat.
func _note_reconnect_failure() -> void:
	if _reconnect_phase == ReconnectPhase.TRYING:
		_reconnect_with_token = false
		_reconnect_deadline = _clock_seconds()


func _give_up_reconnect() -> void:
	_reconnect_phase = ReconnectPhase.IDLE
	last_error_message = "reconnect abandoned after %d attempts" % session_cookie.reconnect_attempts
	room_error.emit(last_error_code, last_error_message)


func _clock_seconds() -> float:
	return Time.get_ticks_msec() / 1000.0


func _handle_server_error(detail: String) -> void:
	_note_reconnect_failure()
	var code := 0
	var message := detail
	var parsed := JSON.parse_string(detail)
	if typeof(parsed) == TYPE_DICTIONARY:
		code = int(parsed.get("code", 0))
		message = str(parsed.get("message", detail))
	last_error_code = code
	last_error_message = message
	room_error.emit(code, message)


func _handle_seat_failed(error: String) -> void:
	_note_reconnect_failure()
	disconnected.emit("matchmake failed: " + error)


func _clear_room_state() -> void:
	_join_handshake_complete = false
	local_player = null
	players.clear()
	_view_index = 0
	latency_ms = 0.0
	_view_table.clear()
	# A rejoin restarts the authority's tick numbering, so the clock must drop its
	# anchor instead of predicting against the previous session's timeline.
	network_time.reset()


func _acquire_instance(scene: PackedScene, position: Vector3, rotation: Quaternion) -> CYGView:
	var holder := scene.instantiate() as Node
	if holder == null:
		push_error("CYGNetwork: prefab scene has no Node root.")
		return null
	var root := spawn_root if spawn_root != null and is_instance_valid(spawn_root) else get_tree().current_scene
	root.add_child(holder)
	# The global pose is only meaningful once the node is in the tree; setting it
	# before `add_child` reads an identity xform and silently spawns at the origin.
	var node_3d := holder as Node3D
	if node_3d != null:
		node_3d.global_position = position
		node_3d.global_rotation = rotation.get_euler()
	var view := holder.get_node_or_null("CYGView") as CYGView
	if view == null:
		view = CYGView.new()
		view.name = "CYGView"
		holder.add_child(view)
	return view


func _release_instance(view: CYGView) -> void:
	view.invoke_despawn_hooks()
	var node := view.network_node
	if node != null:
		node.queue_free()


## Event payloads arrive either msgpack-framed (`[0xA0|n]` / `0xD9` / `0xDA`)
## or with CYG's own LEB128 length prefix. Returns `{"name", "body"}`; an empty
## name means the payload is not a named event.
func _split_event_name(payload: PackedByteArray) -> Dictionary:
	if payload.is_empty():
		return {"name": "", "body": payload}

	var marker := payload[0]
	var name_length := 0
	var header_size := 0

	if marker >= 0xA0 and marker <= 0xBF:
		name_length = marker & 0x1F
		header_size = 1
	elif marker == 0xD9:
		if payload.size() < 2:
			return {"name": "", "body": payload}
		name_length = payload[1]
		header_size = 2
	elif marker == 0xDA:
		if payload.size() < 3:
			return {"name": "", "body": payload}
		name_length = (int(payload[1]) << 8) | int(payload[2])
		header_size = 3
	elif marker < 0x80:
		var reader := CYGBitReader.new()
		reader.attach(payload)
		name_length = reader.read_uint32()
		header_size = reader.bytes_read
	else:
		return {"name": "", "body": payload}

	if name_length <= 0 or name_length > MAX_EVENT_NAME_BYTES \
			or header_size + name_length > payload.size():
		return {"name": "", "body": payload}

	# Binary room payloads share this channel, so a name must look like a name.
	for i in range(name_length):
		var character := payload[header_size + i]
		if character < 0x20 or character > 0x7E:
			return {"name": "", "body": payload}

	var name := payload.slice(header_size, header_size + name_length).get_string_from_utf8()
	return {"name": name, "body": payload.slice(header_size + name_length)}


## FNV-1a 32 of the session id, masked into the 14-bit actor space, matching
## the Unity SDK so both engines derive the same actor for the same seat.
func _derive_actor_number(session_id: String) -> int:
	if session_id.is_empty():
		return 1
	var hash := 2166136261
	var raw := session_id.to_utf8_buffer()
	for i in range(raw.size()):
		hash ^= raw[i]
		hash = (hash * 16777619) & 0xFFFFFFFF
	return int(hash & 0x3FFF) + 1
