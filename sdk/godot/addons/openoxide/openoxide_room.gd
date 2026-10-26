class_name OpenOxideRoom
extends RefCounted
## One joined OpenOxide room — the GDScript port of `OpenOxideRoom` from
## `sdk/web/openoxide.js`.
##
## Owns the WebSocket, the typed app-message lane, the stamped server delta
## channel, schema state, ping/RTT stats, interpolation and recording.
##
## Transport note: the JS SDK prefers WebRTC DataChannels and falls back to
## WebSocket; native Godot builds without a WebRTC extension only offer the
## WebSocket path, which carries the identical frames. Voice stays relay-only
## (`send_voice` / `voice_received`) for the same reason.
##
## Polling: a room must be polled once per frame (`poll()`), which
## `OpenOxideClient` does automatically for rooms it created. A room built by
## hand is polled by its owner, or through `wait_opened()`.

signal opened
signal closed(code: int, reason: String)
signal connection_failed(message: String)
signal server_error(message: String)
signal delta_received(payload: PackedByteArray, acked_seq: int, server_tick: int)
signal event_received(event: Dictionary)
signal app_message_received(type: String, payload: PackedByteArray)
signal schema_applied(kind: int)
signal state_changed(state: OpenOxideSchemaMap)
signal schema_error_raised(message: String)
signal session_id_changed(session_id: int)
signal voice_received(bytes: PackedByteArray)
signal signal_received(frame: Dictionary)
signal reconnecting(attempt: int, delay_ms: int)
signal reconnected
signal reconnect_failed
signal reconnect_error(message: String)

var endpoint: String
var room_name: String
var options: Dictionary
var is_spectator := false

## Numeric session id confirmed by the first delta (JS `sessionId`).
var session_id: Variant = null
## Convenience mirror of `session_id`.
var player_id: Variant = null
var is_open := false
var bytes_in := 0
var bytes_out := 0

## Matchmaking / resume.
var seat_session_id: Variant = null
var reconnection_token: Variant = null
var room_id := ""
var auto_reconnect := true
var max_reconnect_attempts := 5
var client: Variant = null

## Schema state.
var schema: OpenOxideSchemaDef = null
var schemas: Array = []
var state: OpenOxideSchemaMap
var delta_seq := 0
var delta_tick := 0
var stale_deltas := 0
var reassembled_deltas := 0
var expired_deltas := 0
var last_delta_arrival_ms := -1.0
var last_applied_tick := -1

## Reconciliation tail from the last applied delta.
var acked_seq := -1
var server_tick := -1

## Ping / RTT (Lightyear PingManager-lite shape).
var ping_seq := 0
var rtt := 0
var jitter := 0
var rtt_min := 0
var rtt_max := 0
var jitter_buffer_enabled := false
var jitter_delay_ms := 80.0
var auto_jitter_buffer := true
var interpolation_enabled := false
var timeline: OpenOxideInterpolationTimeline = null
var pose_buffer_max := 64
var extrap_max_age_ms := 100.0

var time_sync: OpenOxideTimeSync
var tick_rate := 128
var clock_offset_ms := 0.0
var tick_origin_ms := -1.0
var input_seq := 0
var event_seq := 0

## Recording (`oxr-v1`).
var recording := false
var record_start_time := 0.0
var recorded_frames: Array = []

## Attached voice session (relay-only on native Godot), or null.
var voice: Variant = null

var _ws: WebSocketPeer = null
var _ws_started := false
var _socket_open := false
var _connection_failed := false
var _has_connected := false
var _intentional_close := false
var _leave_code: Variant = null
var _reconnect_attempts := 0
var _reconnect_pending := false
var _url := ""

var _root_collection: Variant = null
var _collection_maps: Dictionary = {}
var _collections: Array = []
var _root_scalars: OpenOxideSchemaDef = null
var _root_values: Dictionary = {}
var _schema_ready := false
var _schema_acked := false
var _schema_error := ""
var _is_triggering := false
var _entity_callbacks: Dictionary = {}  # entity -> { fields: {}, any: [] }
var _schema_callbacks: OpenOxideSchemaCallbacks = null

var _on_add_handlers: Array[Callable] = []
var _on_remove_handlers: Array[Callable] = []
var _on_change_handlers: Array[Callable] = []
var _on_collection_change_handlers: Array[Callable] = []
var _on_state_change_handlers: Array[Callable] = []
var _listeners: Dictionary = {}         # event -> Array[Callable]
var _message_handlers: Dictionary = {}  # type -> Array[Callable]

var _deltas: OpenOxideCodec.DeltaSequencer
var _pending_pings: Dictionary = {}     # seq -> sent ms
var _pending_ping_callbacks: Dictionary = {}  # seq -> Callable
var _ping_window: Array = []
var _ping_loop_interval_ms := 1000
var _ping_loop_running := false
var _jitter_queue: Array = []

var _pose_buffers: Dictionary = {}  # id -> Array[{tick, pose}]
var _pose_scratch: Dictionary = {}  # id -> Dictionary
var _delta_decoder: Callable


func _init(room_endpoint: String = "", name: String = "", room_options: Dictionary = {}) -> void:
	var normalized := room_endpoint.strip_edges().trim_suffix("/")
	if normalized.begins_with("http://"):
		normalized = "ws://" + normalized.substr(7)
	elif normalized.begins_with("https://"):
		normalized = "wss://" + normalized.substr(8)
	endpoint = normalized
	room_name = name
	options = room_options
	is_spectator = bool(options.get("spectator", false))
	seat_session_id = options.get("sessionId", null)
	reconnection_token = options.get("reconnectionToken", null)
	room_id = String(options.get("roomId", name))
	client = options.get("client", null)
	auto_reconnect = bool(options.get("autoReconnect", true))
	max_reconnect_attempts = int(options.get("maxReconnectAttempts", 5))
	state = OpenOxideSchemaMap.new(self)
	time_sync = OpenOxideTimeSync.new()
	time_sync.set_tick_rate(tick_rate)
	_deltas = OpenOxideCodec.DeltaSequencer.new()


func _now_ms() -> float:
	return float(Time.get_ticks_msec())


## Primary (first) collection name from the descriptor.
var root_collection: Variant:
	get:
		return _root_collection


var is_connected: bool:
	get:
		return is_open


func get_session_id() -> Variant:
	return session_id


## -------------------------------------------------------------- connection

## Opens the room socket. JS calls this `connect()`; `Object.connect` is taken
## in GDScript, hence the longer name.
func connect_room() -> bool:
	_intentional_close = false
	_connection_failed = false
	_reconnect_pending = false
	_url = _build_url()
	_ws = WebSocketPeer.new()
	_ws.inbound_buffer_size = 1 << 20
	_ws.outbound_buffer_size = 1 << 20
	var err := _ws.connect_to_url(_url)
	if err != OK:
		_ws = null
		_connection_failed = true
		connection_failed.emit("could not open the socket: %s" % error_string(err))
		_emit("error", [{"message": "could not open the socket", "code": err}])
		return false
	_ws_started = true
	return true


func _build_url() -> String:
	var clean := endpoint if endpoint.ends_with("/") else endpoint + "/"
	var url := clean + room_name
	var query: Array[String] = []
	var seat := seat_session_id if seat_session_id != null else session_id
	if seat != null:
		query.append("sessionId=%s" % String(seat).uri_encode())
	if is_spectator:
		query.append("spectator=1")
	var join_options := {}
	for key in options.keys():
		if key in ["client", "spectator", "sessionId", "reconnectionToken",
				"autoReconnect", "maxReconnectAttempts", "webrtc"]:
			continue
		join_options[key] = options[key]
	if not join_options.is_empty():
		query.append("options=%s" % JSON.stringify(join_options).uri_encode())
	if not query.is_empty():
		url += "?" + "&".join(query)
	return url


## Drives socket I/O, inbound dispatch and time-based delivery. Call per frame.
func poll() -> void:
	if _ws == null:
		return
	_ws.poll()
	var ws_state := _ws.get_ready_state()
	match ws_state:
		WebSocketPeer.STATE_CONNECTING:
			pass
		WebSocketPeer.STATE_OPEN:
			if not _socket_open:
				_socket_open = true
				_handle_socket_open()
			var available := _ws.get_available_packet_count()
			for i in range(available):
				var packet := _ws.get_packet()
				if not _ws.was_string_packet():
					_handle_inbound_binary(packet)
		WebSocketPeer.STATE_CLOSING:
			pass
		WebSocketPeer.STATE_CLOSED:
			if _ws_started:
				_ws_started = false
				_socket_open = false
				var was_open := is_open
				is_open = false
				var code := _ws.get_close_code()
				var reason := _ws.get_close_reason()
				_ws = null
				_cleanup_transport()
				_connection_failed = not was_open
				closed.emit(code, reason)
				_emit("close", [{"code": code, "reason": reason}])
				if not was_open:
					connection_failed.emit("socket closed before it opened")
				_maybe_auto_reconnect(code, reason)
	_drain_jitter_queue()


func _handle_socket_open() -> void:
	is_open = true
	_has_connected = true
	_reconnect_attempts = 0
	_reconnect_pending = false
	if is_spectator:
		_send_wire(PackedByteArray([0xFF, 0x53, 0x50, 0x45, 0x43]))
	opened.emit()
	_emit("open", [])


func _cleanup_transport() -> void:
	_ping_loop_running = false
	_pending_pings.clear()
	_pending_ping_callbacks.clear()
	_jitter_queue.clear()


## Blocks the calling coroutine until the room opens (or fails).
func wait_opened(timeout_seconds := 10.0) -> bool:
	if is_open:
		return true
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		return false
	var deadline := Time.get_ticks_msec() + int(timeout_seconds * 1000.0)
	while Time.get_ticks_msec() < deadline:
		poll()
		if is_open:
			return true
		if _connection_failed:
			return false
		await tree.process_frame
	return false


func leave(consented := true) -> void:
	_leave_code = 1000 if consented else 1001
	_intentional_close = true
	_reconnect_pending = false
	_ping_loop_running = false
	if voice != null and voice.has_method("dispose"):
		voice.dispose()
	if _ws != null:
		_ws.close(_leave_code, "leave")
	_cleanup_transport()


## ------------------------------------------------------------- inbound

func _handle_inbound_binary(bytes: PackedByteArray) -> void:
	bytes_in += bytes.size()
	if bytes.is_empty():
		return

	if OpenOxideSchemaDef.is_descriptor(bytes):
		_attach_descriptor(bytes)
		return

	if int(bytes[0]) == OpenOxideCodec.ChannelOp.DELTA and bytes.size() >= 9:
		_handle_delta_frame(bytes)
		return

	if bytes.size() >= 12 and int(bytes[0]) == 0x11 and int(bytes[1]) == OpenOxideCodec.SIGNAL_MAGIC:
		_handle_signal(bytes.slice(1))
		return
	if bytes.size() >= OpenOxideCodec.SIGNAL_HEADER_LEN and int(bytes[0]) == OpenOxideCodec.SIGNAL_MAGIC:
		_handle_signal(bytes)
		return

	if bytes.size() >= 16 and int(bytes[0]) == 0x11 and int(bytes[1]) == OpenOxideCodec.VOICE_MAGIC:
		_emit_voice(bytes.slice(1))
		return
	if bytes.size() >= 15 and int(bytes[0]) == OpenOxideCodec.VOICE_MAGIC:
		_emit_voice(bytes)
		return

	var event_bytes := PackedByteArray()
	if bytes.size() == 15 and int(bytes[0]) == 0x11 and int(bytes[1]) == OpenOxideCodec.MeshOp.EVENT:
		event_bytes = bytes.slice(1)
	elif bytes.size() == 14 and int(bytes[0]) == OpenOxideCodec.MeshOp.EVENT:
		event_bytes = bytes
	if not event_bytes.is_empty():
		var event := OpenOxideCodec.decode_mesh_frame(event_bytes)
		if not event.is_empty():
			_ack_event(int(event["seq"]))
			event_received.emit(event)
			_emit("event", [event])
			return

	if bytes.size() >= 3 and int(bytes[0]) == 0x11 and int(bytes[1]) == OpenOxideCodec.AppOp.MESSAGE:
		_handle_app_message(bytes.slice(1))
		return
	if bytes.size() >= 3 and int(bytes[0]) == OpenOxideCodec.AppOp.MESSAGE:
		_handle_app_message(bytes)
		return

	if bytes.size() == 5 and int(bytes[0]) == 18:
		var seq := bytes.decode_u32(1)
		if _pending_pings.has(seq):
			var sent_at := float(_pending_pings[seq])
			_pending_pings.erase(seq)
			_on_ping_echo(seq, sent_at, _now_ms())
		return

	var offset := 0
	if bytes.size() >= 15 and int(bytes[0]) == 0x11 and int(bytes[1]) == 0xA5:
		offset = 7
	elif bytes.size() >= 14 and int(bytes[0]) == 0xA5 and int(bytes[1]) == 0x64:
		offset = 6
	if offset > 0:
		_process_delta_payload(bytes.slice(offset))


func _handle_app_message(frame: PackedByteArray) -> void:
	if frame.size() < 3:
		return
	var type_length := int(frame[1])
	if type_length == 0 or 2 + type_length > frame.size():
		return
	var message_type := frame.slice(2, 2 + type_length).get_string_from_utf8()
	var payload := frame.slice(2 + type_length)
	app_message_received.emit(message_type, payload)
	_emit("message", [message_type, payload])
	if _message_handlers.has("*"):
		for handler in _message_handlers["*"]:
			handler.call(message_type, payload, self)
	if _message_handlers.has(message_type):
		for handler in _message_handlers[message_type]:
			handler.call(payload, self)


func _handle_delta_frame(bytes: PackedByteArray) -> void:
	var frame := OpenOxideCodec.parse_delta_frame(bytes)
	if frame.is_empty():
		return
	var delta := _deltas.push(frame)
	stale_deltas = _deltas.stale_count
	expired_deltas = _deltas.expired_count
	if delta.is_empty():
		return
	reassembled_deltas = _deltas.assembled_count
	delta_seq = int(delta["seq"])
	delta_tick = int(delta["tick"])
	_ack_event(delta_seq)
	_process_delta_payload(delta["body"])


func delta_stats() -> Dictionary:
	return {
		"reassembled": _deltas.assembled_count,
		"stale": _deltas.stale_count,
		"expired": _deltas.expired_count,
		"inFlight": _deltas.partials.size(),
	}


func _set_session_id(value: Variant) -> void:
	if value == null:
		return
	var numeric := int(value)
	if session_id != null and int(session_id) == numeric:
		return
	session_id = numeric
	player_id = numeric
	session_id_changed.emit(numeric)
	_emit("sessionid", [numeric])


func _process_delta_payload(raw: PackedByteArray) -> void:
	var payload := raw
	if payload.size() >= 14 and int(payload[0]) == 0xA5 and int(payload[1]) == 0x64 and int(payload[2]) == 0x65:
		payload = payload.slice(6)  # 0xA5 'd' 'e' 'l' 't' 'a'

	# Schema frame: [your_id i64]["OXS2"][kind...][acked u16][tick u32]
	if payload.size() >= 8 + 9 + OpenOxideCodec.DELTA_TAIL_LEN \
			and int(payload[8]) == OpenOxideCodec.SCHEMA_MAGIC[0] \
			and int(payload[9]) == OpenOxideCodec.SCHEMA_MAGIC[1] \
			and int(payload[10]) == OpenOxideCodec.SCHEMA_MAGIC[2] \
			and int(payload[11]) == OpenOxideCodec.SCHEMA_MAGIC[3]:
		var tail := payload.size() - OpenOxideCodec.DELTA_TAIL_LEN
		var acked := payload.decode_u16(tail)
		var tick := payload.decode_u32(tail + 2)
		acked_seq = acked
		server_tick = tick
		if tick <= last_applied_tick:
			return
		last_applied_tick = tick
		_set_session_id(payload.decode_s64(0))
		var arrival := _now_ms()
		last_delta_arrival_ms = arrival
		time_sync.set_tick_rate(tick_rate)
		time_sync.add_sample(arrival, float(tick))
		tick_origin_ms = time_sync.tick_origin_ms
		clock_offset_ms = time_sync.clock_offset_ms(arrival)
		_apply_schema(payload.slice(8, tail))
		return

	var legacy_acked := -1
	var legacy_tick := -1
	if payload.size() >= 23 and (int(payload[0]) == 0xA5 or int(payload[8]) == 1):
		legacy_acked = payload.decode_u16(payload.size() - OpenOxideCodec.DELTA_TAIL_LEN)
		legacy_tick = payload.decode_u32(payload.size() - OpenOxideCodec.DELTA_TAIL_LEN + 2)
		acked_seq = legacy_acked
		server_tick = legacy_tick
		payload = payload.slice(0, payload.size() - OpenOxideCodec.DELTA_TAIL_LEN)

	if legacy_tick >= 0 and legacy_tick <= last_applied_tick:
		return

	if payload.size() >= 8:
		_set_session_id(payload.decode_s64(0))

	if payload.size() >= 15 and int(payload[0]) == OpenOxideCodec.VOICE_MAGIC:
		_emit_voice(payload)
		return

	if recording:
		recorded_frames.append({
			"t": int(round(_now_ms() - record_start_time)),
			"d": Array(payload),
		})

	var arrival_ms := _now_ms()
	last_delta_arrival_ms = arrival_ms
	if legacy_tick >= 0:
		last_applied_tick = legacy_tick
		time_sync.set_tick_rate(tick_rate)
		time_sync.add_sample(arrival_ms, float(legacy_tick))
		tick_origin_ms = time_sync.tick_origin_ms
		clock_offset_ms = time_sync.clock_offset_ms(arrival_ms)

	if timeline != null and _delta_decoder.is_valid() and legacy_tick >= 0:
		var decoded: Variant = _delta_decoder.call(payload)
		if decoded != null:
			timeline.push_snapshot(legacy_tick, decoded, arrival_ms)

	if jitter_buffer_enabled:
		_jitter_queue.append({
			"payload": payload,
			"ackedSeq": legacy_acked,
			"serverTick": legacy_tick,
			"arrivalMs": arrival_ms,
			"deliverAt": arrival_ms + jitter_delay_ms,
		})
	else:
		_emit_delta(payload, legacy_acked, legacy_tick)


func _emit_delta(payload: PackedByteArray, acked: int, tick: int) -> void:
	delta_received.emit(payload, acked, tick)
	_emit("delta", [payload, acked, tick])


func _emit_voice(bytes: PackedByteArray) -> void:
	voice_received.emit(bytes)
	_emit("voice", [bytes])


func _drain_jitter_queue() -> void:
	if not jitter_buffer_enabled or _jitter_queue.is_empty():
		return
	var now := _now_ms()
	while not _jitter_queue.is_empty() and float(_jitter_queue[0]["deliverAt"]) <= now:
		var item: Dictionary = _jitter_queue.pop_front()
		last_delta_arrival_ms = float(item["arrivalMs"])
		_emit_delta(item["payload"], int(item["ackedSeq"]), int(item["serverTick"]))


## --------------------------------------------------------------- sending

func _send_wire(bytes: PackedByteArray) -> bool:
	if _ws == null or not _socket_open:
		return false
	if _ws.put_packet(bytes) != OK:
		return false
	bytes_out += bytes.size()
	return true


func _ack_event(seq: int) -> void:
	_send_wire(OpenOxideCodec.encode_ack(seq, OpenOxideCodec.AckKind.EVENT))


## Typed app message to the server. `data` may be null, a String (UTF-8), a
## PackedByteArray, or anything JSON-serializable.
func send(type: String, data: Variant = null) -> bool:
	var frame := OpenOxideCodec.encode_app_message(type, data)
	if frame.is_empty():
		return false
	return _send_wire(frame)


## No unreliable lane exists on a WebSocket-only transport; returns false.
func send_unreliable(_type: String, _data: Variant = null) -> bool:
	return false


func send_bytes(type: String, bytes: PackedByteArray) -> bool:
	return send(type, bytes)


func send_json(type: String, value: Variant) -> bool:
	return send(type, JSON.stringify(value))


func send_raw(bytes: PackedByteArray) -> bool:
	return _send_wire(bytes)


func on_message(type: String, callback: Callable) -> OpenOxideRoom:
	if type.is_empty() or not callback.is_valid():
		return self
	if not _message_handlers.has(type):
		_message_handlers[type] = []
	_message_handlers[type].append(callback)
	return self


## Stream a 23-byte input frame. `flags & 16` also fires the reliable FIRE pulse.
func send_input(flags: int, aim_yaw: float, aim_pitch: float, move_x: float, move_z: float) -> void:
	if is_spectator:
		return
	input_seq = (input_seq + 1) & 0xFFFF
	var frame := OpenOxideCodec.encode_input_frame({
		"seq": input_seq,
		"flags": flags,
		"aimYaw": aim_yaw,
		"aimPitch": aim_pitch,
		"moveX": move_x,
		"moveZ": move_z,
	})
	if _send_wire(frame) and (flags & 16) != 0:
		send_event(1, 0xFFFF, 0, 1.0)


## Stream a 14-byte reliable event frame.
func send_event(kind: int, target: int = 0xFFFF, tick: int = 0, value: float = 0.0) -> void:
	if is_spectator:
		return
	event_seq = (event_seq + 1) & 0xFFFF
	_send_wire(OpenOxideCodec.encode_event_frame({
		"seq": event_seq,
		"kind": kind,
		"from": int(session_id) if session_id != null else 0,
		"target": target,
		"tick": tick,
		"value": value,
	}))


## 15-byte header + Opus payload, for the server-relayed voice plane.
func send_voice(channel_mode: int, seq: int, timestamp: int, flags: int,
		opus_payload: PackedByteArray) -> void:
	if is_spectator or _ws == null or not _socket_open:
		return
	var out := PackedByteArray()
	out.resize(15 + opus_payload.size())
	out.fill(0)
	out.encode_u8(0, OpenOxideCodec.VOICE_MAGIC)
	out[1] = 0
	out[2] = 0
	out[3] = 0
	out[4] = 0
	out[5] = (seq >> 8) & 0xFF
	out[6] = seq & 0xFF
	out[7] = (timestamp >> 24) & 0xFF
	out[8] = (timestamp >> 16) & 0xFF
	out[9] = (timestamp >> 8) & 0xFF
	out[10] = timestamp & 0xFF
	out[11] = channel_mode & 0xFF
	out[12] = flags & 0xFF
	out[13] = (opus_payload.size() >> 8) & 0xFF
	out[14] = opus_payload.size() & 0xFF
	for i in range(opus_payload.size()):
		out[15 + i] = opus_payload[i]
	_send_wire(out)


func set_voice_team(team_id: int) -> void:
	_send_wire(PackedByteArray([OpenOxideCodec.VOICE_MAGIC, 0x54, team_id & 0xFF]))


func set_voice_muted(is_muted: bool) -> void:
	_send_wire(PackedByteArray([OpenOxideCodec.VOICE_MAGIC, 0x4D, 1 if is_muted else 0]))


func set_voice_deafened(is_deafened: bool) -> void:
	_send_wire(PackedByteArray([OpenOxideCodec.VOICE_MAGIC, 0x44, 1 if is_deafened else 0]))


func send_signal(kind: int, to: int, body: Variant = null) -> bool:
	if _ws == null or not _socket_open:
		return false
	return _send_wire(OpenOxideCodec.encode_signal_frame(kind, to, body))


func _handle_signal(bytes: PackedByteArray) -> void:
	var frame := OpenOxideCodec.decode_signal_frame(bytes)
	if frame.is_empty():
		return
	if voice != null and voice.has_method("_on_signal"):
		voice._on_signal(frame)
	signal_received.emit(frame)
	_emit("signal", [frame])


## Attaches the relay-only voice session (P2P mesh needs a WebRTC extension).
func attach_voice(voice_options: Dictionary = {}) -> OpenOxideVoice:
	voice = OpenOxideVoice.new(self, voice_options)
	return voice


## --------------------------------------------------------------- ping

func ping(callback: Callable = Callable()) -> int:
	if _ws == null or not _socket_open:
		if callback.is_valid():
			callback.call(-1)
		return -1
	ping_seq = (ping_seq + 1) & 0xFFFFFFFF
	var sent_at := _now_ms()
	_pending_pings[ping_seq] = sent_at
	if callback.is_valid():
		_pending_ping_callbacks[ping_seq] = callback
	var frame := PackedByteArray()
	frame.resize(5)
	frame.fill(0)
	frame.encode_u8(0, 18)
	frame.encode_u32(1, ping_seq)
	_send_wire(frame)
	var tree := Engine.get_main_loop() as SceneTree
	if tree != null:
		tree.create_timer(2.0).timeout.connect(_on_ping_timeout.bind(ping_seq), CONNECT_ONE_SHOT)
	return ping_seq


func _on_ping_timeout(seq: int) -> void:
	if not _pending_pings.has(seq):
		return
	_pending_pings.erase(seq)
	if _pending_ping_callbacks.has(seq):
		var callback: Callable = _pending_ping_callbacks[seq]
		_pending_ping_callbacks.erase(seq)
		if callback.is_valid():
			callback.call(-1)


func _on_ping_echo(seq: int, sent_at: float, now: float) -> void:
	var sample := now - sent_at
	_ping_window.append(sample)
	if _ping_window.size() > 30:
		_ping_window.pop_front()
	var total := 0.0
	var smallest := INF
	var largest := -INF
	for value in _ping_window:
		total += value
		smallest = minf(smallest, value)
		largest = maxf(largest, value)
	var average := total / float(_ping_window.size())
	var deviation := 0.0
	for value in _ping_window:
		deviation += absf(value - average)
	deviation /= float(_ping_window.size())
	rtt_min = int(round(smallest))
	rtt_max = int(round(largest))
	jitter = int(round(deviation))
	var weight := 1.0 if rtt == 0 else 0.3
	rtt = int(round(average if rtt == 0 else rtt * (1.0 - weight) + average * weight))
	if auto_jitter_buffer:
		jitter_delay_ms = float(clampi(int(round(2.0 * jitter + 8.0)), 20, 250))
	if _pending_ping_callbacks.has(seq):
		var callback: Callable = _pending_ping_callbacks[seq]
		_pending_ping_callbacks.erase(seq)
		if callback.is_valid():
			callback.call(sample)


func start_ping_loop(interval_ms: int = 1000) -> void:
	_ping_loop_interval_ms = interval_ms
	stop_ping_loop()
	_ping_loop_running = true
	ping()
	_schedule_ping_tick()


func _schedule_ping_tick() -> void:
	if not _ping_loop_running:
		return
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		return
	tree.create_timer(float(_ping_loop_interval_ms) / 1000.0).timeout.connect(_ping_tick, CONNECT_ONE_SHOT)


func _ping_tick() -> void:
	if not _ping_loop_running:
		return
	if _pending_pings.is_empty():
		ping()
	_schedule_ping_tick()


func stop_ping_loop() -> void:
	_ping_loop_running = false


## ----------------------------------------------------- jitter / clocks

func enable_jitter_buffer(delay_ms: float = -1.0) -> void:
	jitter_buffer_enabled = true
	if delay_ms >= 0.0:
		jitter_delay_ms = delay_ms
		auto_jitter_buffer = false
	elif not auto_jitter_buffer:
		jitter_delay_ms = jitter_delay_ms if jitter_delay_ms > 0.0 else 80.0


func disable_jitter_buffer() -> void:
	jitter_buffer_enabled = false
	var queued := _jitter_queue
	_jitter_queue = []
	for item in queued:
		_emit_delta(item["payload"], int(item["ackedSeq"]), int(item["serverTick"]))


func enable_interpolation(interp_options: Dictionary = {},
		decoder: Callable = Callable()) -> OpenOxideInterpolationTimeline:
	timeline = OpenOxideInterpolationTimeline.new(interp_options)
	interpolation_enabled = true
	if decoder.is_valid():
		_delta_decoder = decoder
	return timeline


func disable_interpolation() -> void:
	interpolation_enabled = false
	timeline = null
	_delta_decoder = Callable()


func push_interpolation(tick: int, entities: Variant, arrival_ms: float = -1.0) -> Variant:
	if timeline == null:
		return null
	timeline.push_snapshot(tick, entities, arrival_ms if arrival_ms >= 0.0 else last_delta_arrival_ms)
	return timeline


func sample_interpolation(now_ms: float = -1.0) -> Variant:
	return timeline.sample(now_ms) if timeline != null else null


func interpolation_stats() -> Variant:
	return timeline.stats() if timeline != null else null


func server_now() -> float:
	var now := _now_ms()
	return now + clock_offset_ms if time_sync.synced else now


func tick_to_time_ms(tick: float) -> float:
	return time_sync.tick_to_time_ms(tick)


func time_to_tick(t: float) -> float:
	return time_sync.time_to_tick(t)


## Record one pose snapshot for `id` (<= `pose_buffer_max` entries per entity).
func buffer_state(id: Variant, server_tick_value: int, pose: Dictionary) -> void:
	if not is_finite(float(server_tick_value)):
		return
	if not _pose_buffers.has(id):
		_pose_buffers[id] = []
	var buffer: Array = _pose_buffers[id]
	if not buffer.is_empty():
		var last: Dictionary = buffer.back()
		if server_tick_value < int(last["tick"]):
			return
		if server_tick_value == int(last["tick"]):
			_copy_pose(last["pose"], pose)
			return
	if buffer.size() >= pose_buffer_max:
		buffer.pop_front()
	var slot := {"tick": server_tick_value, "pose": {}}
	buffer.append(slot)
	_copy_pose(slot["pose"], pose)


## Interpolated pose for `id` at `render_time_ms`; the returned dictionary is a
## per-id scratch reused on the next call.
func sample_state(id: Variant, render_time_ms: float) -> Variant:
	if not _pose_buffers.has(id) or (_pose_buffers[id] as Array).is_empty():
		return null
	var buffer: Array = _pose_buffers[id]
	var target := time_sync.time_to_tick(render_time_ms)
	var newest: Dictionary = buffer.back()
	var out := _scratch_for(id)
	if target >= float(newest["tick"]):
		var extra_ticks := target - float(newest["tick"])
		if buffer.size() >= 2 and extra_ticks > 0.0:
			var previous: Dictionary = buffer[buffer.size() - 2]
			var gap := float(newest["tick"]) - float(previous["tick"])
			if gap > 0.0:
				var max_extra := extrap_max_age_ms / time_sync.ms_per_tick
				if extra_ticks > max_extra:
					extra_ticks = max_extra
				_blend_pose(previous["pose"], newest["pose"], 1.0 + extra_ticks / gap, out)
				return out
		_copy_pose(out, newest["pose"])
		return out
	if target <= float(buffer[0]["tick"]):
		_copy_pose(out, buffer[0]["pose"])
		return out
	var index := buffer.size() - 1
	while index > 0 and float(buffer[index]["tick"]) > target:
		index -= 1
	var a: Dictionary = buffer[index]
	var b: Dictionary = buffer[index + 1]
	var span := float(b["tick"]) - float(a["tick"])
	var alpha := 0.0 if span == 0.0 else (target - float(a["tick"])) / span
	alpha = clampf(alpha, 0.0, 1.0)
	_blend_pose(a["pose"], b["pose"], alpha, out)
	return out


func forget_entity(id: Variant) -> void:
	_pose_buffers.erase(id)
	_pose_scratch.erase(id)


func _scratch_for(id: Variant) -> Dictionary:
	if not _pose_scratch.has(id):
		_pose_scratch[id] = {}
	return _pose_scratch[id]


static func _copy_pose(out: Dictionary, source: Dictionary) -> void:
	for key in out.keys():
		if not source.has(key):
			out.erase(key)
	for key in source.keys():
		out[key] = source[key]


static func _blend_pose(a: Dictionary, b: Dictionary, alpha: float, out: Dictionary) -> void:
	for key in b.keys():
		out[key] = b[key]
	for key in a.keys():
		var va: Variant = a[key]
		var vb: Variant = b.get(key)
		if (typeof(va) == TYPE_FLOAT or typeof(va) == TYPE_INT) \
				and (typeof(vb) == TYPE_FLOAT or typeof(vb) == TYPE_INT):
			out[key] = float(va) + (float(vb) - float(va)) * alpha
		else:
			out[key] = va


## --------------------------------------------------------------- schema

## Attach a schema definition. Not required against a live host: the room
## attaches the "OXSD" descriptor on join by itself.
func use_schema(schema_def: OpenOxideSchemaDef) -> OpenOxideRoom:
	schema = schema_def
	schemas = [schema_def]
	_root_scalars = null
	_root_values = {}
	var name := String(_root_collection) if _root_collection != null else "players"
	state = OpenOxideSchemaMap.new(self, name, schema_def)
	_collection_maps = {name: state}
	_collections = [{"id": 0, "name": name, "map": state, "schema": schema_def}]
	_entity_callbacks = {}
	_schema_ready = false
	_schema_acked = false
	_schema_error = ""
	return self


## Attach an "OXSD" descriptor locally (live rooms receive it automatically).
func use_descriptor(bytes: PackedByteArray) -> OpenOxideRoom:
	if not OpenOxideSchemaDef.is_descriptor(bytes):
		_schema_error = "use_descriptor() expects an OXSD buffer"
		push_error("[OpenOxide] use_descriptor() expects an OXSD buffer")
		schema_error_raised.emit(_schema_error)
		return self
	_attach_descriptor(bytes)
	return self


func _attach_descriptor(bytes: PackedByteArray) -> void:
	var parsed := OpenOxideSchemaDef.parse_descriptor(bytes)
	var collections: Array = parsed.get("collections", [])
	if parsed.is_empty() or collections.is_empty():
		_fail_schema("[OpenOxide] invalid schema descriptor from server")
		return
	var first: Dictionary = collections[0]
	schemas = parsed["schemas"]
	_root_collection = first["name"]
	_root_scalars = parsed.get("root_scalars")

	if schema != null and collections.size() == 1 and _root_scalars == null \
			and schema.hash == (first["schema"] as OpenOxideSchemaDef).hash:
		schema = first["schema"]
		state._name = first["name"]
		state._schema = first["schema"]
		_collection_maps = {first["name"]: state}
		_collections = [{"id": 0, "name": first["name"], "map": state, "schema": first["schema"]}]
		return

	var old_callbacks: Dictionary = _entity_callbacks.get(state, {})
	schema = first["schema"]
	state = OpenOxideSchemaMap.new(self, first["name"], first["schema"])
	_entity_callbacks = {}
	_collection_maps = {}
	_collections = []
	_root_values = {}
	if _root_scalars != null:
		for field in _root_scalars.fields:
			_root_values[field["name"]] = _schema_default_value(field)
		_track_entity(state)
		if not old_callbacks.is_empty():
			var meta: Dictionary = _entity_callbacks[state]
			for key in old_callbacks.get("fields", {}).keys():
				meta["fields"][key] = old_callbacks["fields"][key]
			meta["any"].append_array(old_callbacks.get("any", []))
	for i in range(collections.size()):
		var entry: Dictionary = collections[i]
		var map: OpenOxideSchemaMap = state if i == 0 else OpenOxideSchemaMap.new(self, entry["name"], entry["schema"])
		_collection_maps[entry["name"]] = map
		_collections.append({"id": i, "name": entry["name"], "map": map, "schema": entry["schema"]})
	_schema_ready = false
	_schema_acked = false
	_schema_error = ""


func _callbacks() -> OpenOxideSchemaCallbacks:
	if _schema_callbacks == null:
		_schema_callbacks = OpenOxideSchemaCallbacks.new(self)
	return _schema_callbacks


func _track_entity(entity: Variant) -> void:
	if not (entity is Object) or _entity_callbacks.has(entity):
		return
	_entity_callbacks[entity] = {"fields": {}, "any": []}


func _fire_add(map: OpenOxideSchemaMap, entity: Variant, id: int) -> void:
	var previous := _is_triggering
	_is_triggering = true
	for handler in map._on_add_handlers:
		handler.call(entity, id)
	for handler in _on_add_handlers:
		handler.call(entity, id)
	_is_triggering = previous


func _fire_remove(map: OpenOxideSchemaMap, entity: Variant, id: int) -> void:
	var previous := _is_triggering
	_is_triggering = true
	for handler in map._on_remove_handlers:
		handler.call(entity, id)
	for handler in _on_remove_handlers:
		handler.call(entity, id)
	_is_triggering = previous


func _fire_field_change(entity: Variant, field: String, value: Variant, previous_value: Variant) -> void:
	var previous := _is_triggering
	_is_triggering = true
	var meta: Dictionary = _entity_callbacks.get(entity, {})
	var listeners: Array = meta.get("fields", {}).get(field, [])
	for handler in listeners:
		handler.call(value, previous_value)
	for handler in _on_change_handlers:
		handler.call(entity, field, value)
	_is_triggering = previous


func _fire_update(map: OpenOxideSchemaMap, entity: Variant, id: int) -> void:
	var previous := _is_triggering
	_is_triggering = true
	var meta: Dictionary = _entity_callbacks.get(entity, {})
	for handler in meta.get("any", []):
		handler.call()
	for handler in map._on_change_handlers:
		handler.call(id, entity)
	for handler in _on_collection_change_handlers:
		handler.call(id, entity)
	_is_triggering = previous


func _emit_state_change() -> void:
	for handler in _on_state_change_handlers:
		handler.call(state)


func on_add(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		_on_add_handlers.append(callback)
	return self


func on_remove(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		_on_remove_handlers.append(callback)
	return self


## Called once per changed field: `(entity, field_name, value)`.
func on_change(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		_on_change_handlers.append(callback)
	return self


func on_state_change(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		_on_state_change_handlers.append(callback)
	return self


func on_error(callback: Callable) -> OpenOxideRoom:
	if not callback.is_valid():
		return self
	on("error", func(err: Variant) -> void:
		var detail: Dictionary = err if typeof(err) == TYPE_DICTIONARY else {"message": str(err)}
		callback.call(int(detail.get("code", 0)), str(detail.get("message", str(err)))))
	return self


func on_reconnect(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		on("reconnect", func() -> void: callback.call())
	return self


func on_drop(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		on("close", func(event: Variant) -> void:
			var detail: Dictionary = event if typeof(event) == TYPE_DICTIONARY else {}
			if auto_reconnect and not _intentional_close:
				callback.call(int(detail.get("code", _leave_code if _leave_code != null else 1006)),
					detail.get("reason", "")))
	return self


func on_leave(callback: Callable) -> OpenOxideRoom:
	if callback.is_valid():
		on("close", func(event: Variant) -> void:
			var detail: Dictionary = event if typeof(event) == TYPE_DICTIONARY else {}
			var code := _leave_code if _leave_code != null else detail.get("code", 1006)
			callback.call(int(code), detail.get("reason", "")))
	return self


func remove_all_listeners() -> OpenOxideRoom:
	_message_handlers.clear()
	_listeners.clear()
	_on_add_handlers.clear()
	_on_remove_handlers.clear()
	_on_change_handlers.clear()
	_on_collection_change_handlers.clear()
	_on_state_change_handlers.clear()
	return self


## ------------------------------------------------------------ schema wire

func _fail_schema(message: String) -> bool:
	_schema_error = message
	schema_error_raised.emit(message)
	_emit("schemaerror", [message])
	return false


func _read_schema_element(reader: OpenOxideCodec.Reader, type_name: String) -> Variant:
	match type_name:
		"u8":
			return reader.read_u8()
		"i8":
			return reader.read_i8()
		"u16":
			return reader.read_u16()
		"i16":
			return reader.read_s16()
		"u32":
			return reader.read_u32()
		"i32":
			return reader.read_s32()
		"f32":
			return reader.read_f32()
		"f64":
			return reader.read_f64()
		"bool":
			return 1 if reader.read_u8() != 0 else 0
		_:
			reader.skip(1)
			return 0


func _read_schema_field(reader: OpenOxideCodec.Reader, field: Dictionary) -> Variant:
	if bool(field["optional"]):
		var present := reader.read_u8() != 0
		if not present:
			return null
	var type_name := String(field["type"])
	if type_name == "string":
		var length := reader.read_u8()
		return reader.read_bytes(length).get_string_from_utf8()
	if type_name == "array":
		var count := reader.read_u8()
		var values: Array = []
		for i in range(count):
			values.append(_read_schema_element(reader, String(field["element"])))
		return values
	match type_name:
		"u8":
			return reader.read_u8()
		"i8":
			return reader.read_i8()
		"u16":
			return reader.read_u16()
		"i16":
			return reader.read_s16()
		"u32":
			return reader.read_u32()
		"i32":
			return reader.read_s32()
		"f32":
			return reader.read_f32()
		"f64":
			return reader.read_f64()
		"bool":
			return reader.read_u8() != 0
		"i64":
			return reader.read_i64()
		_:
			reader.skip(int(field["size"]))
			return 0


func _schema_default_value(field: Dictionary) -> Variant:
	if bool(field["optional"]):
		return null
	match String(field["type"]):
		"string":
			return ""
		"array":
			return []
		"bool":
			return false
		_:
			return 0


func _schema_value_equals(a: Variant, b: Variant) -> bool:
	var type_a := typeof(a)
	var type_b := typeof(b)
	var a_numeric := type_a == TYPE_INT or type_a == TYPE_FLOAT
	var b_numeric := type_b == TYPE_INT or type_b == TYPE_FLOAT
	if a_numeric and b_numeric:
		if is_nan(float(a)) and is_nan(float(b)):
			return true
		return float(a) == float(b)
	if type_a != type_b:
		return false
	if type_a == TYPE_ARRAY:
		var left: Array = a
		var right: Array = b
		if left.size() != right.size():
			return false
		for i in range(left.size()):
			if not _schema_value_equals(left[i], right[i]):
				return false
		return true
	return a == b


func _decode_entity(reader: OpenOxideCodec.Reader, id: int,
		schema_def: OpenOxideSchemaDef) -> OpenOxideSchemaEntity:
	var fields := {}
	for field in schema_def.fields:
		fields[field["name"]] = _read_schema_field(reader, field)
	return OpenOxideSchemaEntity.new(id, fields, self)


func _apply_schema(section: PackedByteArray) -> void:
	if schema == null or _collections.is_empty():
		return
	var kind := int(section[4]) if section.size() > 4 else -1
	var applied := _apply_schema_frame(section)
	if not applied:
		return
	schema_applied.emit(kind)
	_emit("schema", [{"kind": kind, "state": state}])
	_emit_state_change()


func _apply_schema_frame(section: PackedByteArray) -> bool:
	if section.size() < 5:
		return false
	for i in range(4):
		if int(section[i]) != int(OpenOxideCodec.SCHEMA_MAGIC[i]):
			return false
	var kind := int(section[4])
	if kind == 1 or kind == 2:
		if _collections.is_empty():
			return false
		var entry: Dictionary = _collections[0]
		if not _apply_collection_section(entry["map"], entry["schema"], section, kind):
			return false
		if kind == 1:
			entry["map"]._ready = true
			_schema_ready = true
			_ack_schema()
		return true
	if kind == 3 or kind == 4:
		return _apply_world_section(section, kind)
	_fail_schema("[OpenOxide] unknown schema frame kind %d" % kind)
	return false


func _ack_schema() -> void:
	_schema_acked = true
	send("__ox_schema_ready")


## Apply a kind-1 full or kind-2 patch section to one collection.
func _apply_collection_section(map: OpenOxideSchemaMap, schema_def: OpenOxideSchemaDef,
		section: PackedByteArray, kind: int) -> bool:
	if section.size() < 13:
		return _fail_schema("[OpenOxide] truncated schema section")
	for i in range(4):
		if int(section[i]) != int(OpenOxideCodec.SCHEMA_MAGIC[i]):
			return _fail_schema("[OpenOxide] schema section magic mismatch")
	var reader := OpenOxideCodec.Reader.new(section)
	reader.offset = 5
	var hash := reader.read_u32()
	if hash != schema_def.hash:
		return _fail_schema("[OpenOxide] schema hash mismatch: server %x client %x" % [hash, schema_def.hash])
	if kind == 2 and not map._ready:
		return false

	var count := reader.read_u32()
	reader.offset = 13

	if kind == 1:
		# Reconcile instead of clear+rebuild: identity, onRemove and field
		# listeners must survive a reconnect full snapshot.
		var incoming: Array = []
		for i in range(count):
			var id := reader.read_u32()
			var entity := _decode_entity(reader, id, schema_def)
			if not reader.ok:
				return _fail_schema("[OpenOxide] truncated schema section")
			incoming.append([id, entity])
		var incoming_ids := {}
		for pair in incoming:
			incoming_ids[pair[0]] = true
		for id in map.ids().duplicate():
			if not incoming_ids.has(id):
				var removed: OpenOxideSchemaEntity = map.get_entity(id)
				map.erase_entity(id)
				_fire_remove(map, removed, id)
		for pair in incoming:
			var id: int = pair[0]
			var entity: OpenOxideSchemaEntity = pair[1]
			var existing: OpenOxideSchemaEntity = map.get_entity(id)
			if existing == null:
				map.set_entity(id, entity)
				_track_entity(entity)
				_fire_add(map, entity, id)
				continue
			var changed := false
			for field in schema_def.fields:
				var field_name := String(field["name"])
				var value: Variant = entity.get_field(field_name)
				var previous_value: Variant = existing.get_field(field_name)
				if _schema_value_equals(previous_value, value):
					continue
				existing.set_field(field_name, value)
				_fire_field_change(existing, field_name, value, previous_value)
				changed = true
			if changed:
				_fire_update(map, existing, id)
		map._ready = true
		return true

	for i in range(count):
		var op := reader.read_u8()
		if op == 1:  # ADD
			var id := reader.read_u32()
			var entity := _decode_entity(reader, id, schema_def)
			if not reader.ok:
				return _fail_schema("[OpenOxide] truncated schema add")
			map.set_entity(id, entity)
			_track_entity(entity)
			_fire_add(map, entity, id)
		elif op == 2:  # REMOVE
			var id := reader.read_u32()
			var entity: OpenOxideSchemaEntity = map.get_entity(id)
			if entity != null:
				map.erase_entity(id)
				_fire_remove(map, entity, id)
		elif op == 3:  # UPDATE
			var id := reader.read_u32()
			var mask := reader.read_u32()
			var entity: OpenOxideSchemaEntity = map.get_entity(id)
			var changed := false
			for f in range(schema_def.fields.size()):
				if (mask & (1 << f)) == 0:
					continue
				var field: Dictionary = schema_def.fields[f]
				var value: Variant = _read_schema_field(reader, field)
				if not reader.ok:
					return _fail_schema("[OpenOxide] truncated schema update")
				if entity != null:
					var field_name := String(field["name"])
					var previous_value: Variant = entity.get_field(field_name)
					entity.set_field(field_name, value)
					if not _schema_value_equals(previous_value, value):
						_fire_field_change(entity, field_name, value, previous_value)
						changed = true
			if changed and entity != null:
				_fire_update(map, entity, id)
		else:
			return _fail_schema("[OpenOxide] unknown schema op %d" % op)
	map._ready = true
	return true


## Apply a kind-3 full or kind-4 patch multi-collection world frame.
func _apply_world_section(section: PackedByteArray, kind: int) -> bool:
	if section.size() < 11:
		return _fail_schema("[OpenOxide] truncated schema world header")
	if kind == 4 and not _schema_ready:
		return false

	var reader := OpenOxideCodec.Reader.new(section)
	reader.offset = 5
	var root_hash := reader.read_u32()
	var root_count := reader.read_u16()
	var scalars := _root_scalars
	var expected := scalars.fields.size() if scalars != null else 0
	if root_count != expected:
		return _fail_schema("[OpenOxide] root field count mismatch: server %d client %d" % [root_count, expected])
	if scalars != null and root_hash != scalars.hash:
		return _fail_schema("[OpenOxide] root schema hash mismatch: server %x client %x" % [root_hash, scalars.hash])

	var mask := 0
	if kind == 4:
		mask = reader.read_u32()
	for f in range(root_count):
		if kind == 4 and (mask & (1 << f)) == 0:
			continue
		var field: Dictionary = scalars.fields[f]
		var value: Variant = _read_schema_field(reader, field)
		if not reader.ok:
			return _fail_schema("[OpenOxide] truncated root scalar")
		var field_name := String(field["name"])
		var previous_value: Variant = _root_values.get(field_name)
		_root_values[field_name] = value
		if not _schema_value_equals(previous_value, value):
			_fire_field_change(state, field_name, value, previous_value)

	var count := reader.read_u16()
	for i in range(count):
		if reader.offset + 10 > section.size():
			return _fail_schema("[OpenOxide] truncated collection header")
		var id := reader.read_u16()
		var hash := reader.read_u32()
		var length := reader.read_u32()
		if reader.offset + length > section.size():
			return _fail_schema("[OpenOxide] truncated collection section")
		if id >= _collections.size():
			return _fail_schema("[OpenOxide] unknown collection id %d" % id)
		var entry: Dictionary = _collections[id]
		var entry_schema: OpenOxideSchemaDef = entry["schema"]
		if entry_schema.hash != hash:
			return _fail_schema("[OpenOxide] collection '%s' schema hash mismatch: server %x client %x"
				% [entry["name"], hash, entry_schema.hash])
		var collection_section := reader.read_bytes(length)
		if not _apply_collection_section(entry["map"], entry_schema, collection_section,
				1 if kind == 3 else 2):
			return false
		if kind == 3:
			entry["map"]._ready = true
	if reader.offset != section.size():
		return _fail_schema("[OpenOxide] trailing bytes in schema world section")

	if kind == 3:
		_schema_ready = true
		_ack_schema()
	return true


## ------------------------------------------------------------- reconnect

func _maybe_auto_reconnect(code: int, reason: String) -> void:
	if _intentional_close or not auto_reconnect or not _has_connected:
		return
	if reconnection_token == null or client == null:
		return
	if code == 4001 or code == 4002 or code == 4003:
		return
	if _reconnect_pending:
		return
	if _reconnect_attempts >= max_reconnect_attempts:
		reconnect_failed.emit()
		_emit("reconnectfailed", [])
		return
	_reconnect_attempts += 1
	_reconnect_pending = true
	var attempt := _reconnect_attempts
	var delay := mini(500 * int(pow(2, attempt - 1)), 8000)
	reconnecting.emit(attempt, delay)
	_emit("reconnecting", [attempt, delay])
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		_reconnect_pending = false
		return
	tree.create_timer(float(delay) / 1000.0).timeout.connect(
		_attempt_reconnect.bind(attempt, reason), CONNECT_ONE_SHOT)


func _attempt_reconnect(_attempt: int, _reason: String) -> void:
	_reconnect_pending = false
	if _intentional_close or not auto_reconnect:
		return
	if client == null:
		reconnect_error.emit("no client to reserve a seat with")
		_maybe_auto_reconnect(0, "")
		return
	var seat: Dictionary = await client.reserve_seat("reconnect", room_name,
		{"reconnectionToken": reconnection_token})
	if seat.is_empty():
		reconnect_error.emit("reconnect matchmake failed")
		_emit("reconnecterror", ["reconnect matchmake failed"])
		_maybe_auto_reconnect(0, "")
		return
	reconnection_token = seat.get("reconnectionToken", seat.get("seatToken", reconnection_token))
	seat_session_id = seat.get("sessionId", seat_session_id)
	_schema_ready = false
	_schema_acked = false
	if not connect_room():
		reconnect_error.emit("reconnect socket failed")
		_maybe_auto_reconnect(0, "")
		return
	var ok: bool = await wait_opened(10.0)
	if ok:
		reconnected.emit()
		_emit("reconnect", [])
	else:
		reconnect_error.emit("reconnect timed out")
		_maybe_auto_reconnect(0, "")


## ---------------------------------------------------------------- events

func on(event_name: String, callback: Callable) -> OpenOxideRoom:
	if not callback.is_valid():
		return self
	if not _listeners.has(event_name):
		_listeners[event_name] = []
	_listeners[event_name].append(callback)
	return self


func off(event_name: String, callback: Callable = Callable()) -> OpenOxideRoom:
	if not _listeners.has(event_name):
		return self
	if not callback.is_valid():
		_listeners.erase(event_name)
		return self
	var list: Array = _listeners[event_name]
	var index := list.find(callback)
	if index >= 0:
		list.remove_at(index)
	if list.is_empty():
		_listeners.erase(event_name)
	return self


func _emit(event_name: String, args: Array = []) -> void:
	if not _listeners.has(event_name):
		return
	for callback in (_listeners[event_name] as Array).duplicate():
		callback.callv(args)


## Fire a room event listener directly (JS `room.emit(event, ...args)`).
func emit(event_name: String, args: Array = []) -> void:
	_emit(event_name, args)


## ------------------------------------------------------------- recording

func start_recording() -> void:
	recording = true
	record_start_time = _now_ms()
	recorded_frames = []


func stop_recording() -> Dictionary:
	recording = false
	return {
		"format": "oxr-v1",
		"room": room_name,
		"recordedAt": Time.get_datetime_string_from_system(true),
		"durationMs": int(round(_now_ms() - record_start_time)),
		"totalFrames": recorded_frames.size(),
		"frames": recorded_frames,
	}


## Writes the replay to `user://` (Godot has no browser download prompt) and
## returns the same payload `stop_recording()` produced.
func download_replay(filename := "match.oxr") -> Dictionary:
	var data := stop_recording()
	var file := FileAccess.open("user://" + filename, FileAccess.WRITE)
	if file != null:
		file.store_string(JSON.stringify(data))
	return data
