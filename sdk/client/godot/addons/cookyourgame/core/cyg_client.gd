class_name CYGClient
## WebSocket client for the CookYourGame room server.
##
## Owns the socket, the seat/matchmake handshake and the binary deframe layer.
## Every inbound WebSocket message is exactly one protocol message:
## `[wire code][body]`. Messages are normalized into canonical CYG frames
## (`[len u16 BE][op u8][payload]`) and emitted as `frame_received`, mirroring
## `CYGTransport.DeframeMessage` in the Unity SDK:
##
##  * `[17]['C'][frames...]` — tunneled game packets (RPCs, spawns, patches),
##    possibly several per message.
##  * `[11][...]`            — server rejection.
##  * known wire codes       — normalized into one canonical frame.
##  * anything else          — read as raw canonical frames if it parses.

signal frame_received(frame: PackedByteArray)
signal socket_opened
signal socket_closed(reason: String, code: int)
signal server_error(detail: String)
signal seat_ready(seat: CYGTypes.Seat)
signal seat_failed(error: String)

const MAX_WIRE_MESSAGE_SIZE := 65536
const PING_INTERVAL_SECONDS := 5.0

var is_connected: bool = false
var last_close_code: int = 0
var settings: CYGServerSettings

## Callback the autoload installs to timestamp an outbound ping (RTT probe).
var on_ping_sent: Callable

var _peer := WebSocketPeer.new()
var _state := WebSocketPeer.STATE_CLOSED
var _url := ""
var _token := ""
var _ping_writer := CYGBitPacker.new()
var _outbound: Array[PackedByteArray] = []
var _last_ping_at := 0.0
var _ping_seq := 0.0
var _seat_requesting := false
var _seat := CYGTypes.Seat.new()


func session_id() -> String:
	return _seat.session_id


func reconnection_token() -> String:
	return _seat.reconnection_token


## Token presented in the join handshake body (empty for direct urls).
func session_token() -> String:
	return _token if not _token.is_empty() else _seat.reconnection_token


## Acknowledges the server-initiated JOIN_ROOM handshake with a bare [10].
func ack_join() -> void:
	send_wire(CYGOpcodes.WireJoinRoom, PackedByteArray())


## Reserves a seat over HTTP, then opens the game socket for it.
func connect_to_room(room_name: String, player_name: String = "") -> void:
	if is_connected or _seat_requesting:
		return
	if settings == null:
		push_error("CYGClient: settings must be assigned before connecting.")
		seat_failed.emit("no server settings")
		return

	var room := CYGServerSettings.normalize_room_name(room_name)
	var url := settings.matchmake_url(settings.matchmake_method, room)
	if url.is_empty():
		seat_failed.emit("bad matchmake url")
		return
	_seat_requesting = true

	var body := JSON.stringify({
		"playerName": player_name if not player_name.is_empty() else "Player",
		"maxPlayers": settings.max_players,
		"protocolVersion": settings.protocol_version,
	})
	_request_seat(url, body)


## Resumes a dropped seat with a previously issued reconnection token.
func reconnect_to_room(room_name: String, reconnection_token: String) -> void:
	if is_connected or _seat_requesting:
		return
	if settings == null:
		return
	_seat_requesting = true
	var url := settings.matchmake_url("reconnect", CYGServerSettings.normalize_room_name(room_name))
	_request_seat(url, JSON.stringify({"reconnectionToken": reconnection_token}))


func _request_seat(url: String, body: String) -> void:
	var node := get_parent_of_requested_node()
	if node == null:
		_seat_requesting = false
		push_error("CYGClient: no node in the tree can host the seat request.")
		seat_failed.emit("no http host")
		return

	var http := HTTPRequest.new()
	http.timeout = 10.0
	node.add_child(http)
	var start := http.request(url, ["Content-Type: application/json"], HTTPClient.METHOD_POST, body)
	if start != OK:
		http.queue_free()
		_seat_requesting = false
		push_warning("CYGClient: matchmake request could not start (%d)." % start)
		seat_failed.emit("matchmake could not start: %d" % start)
		return

	# Godot 4's HTTPRequest reports through `request_completed`, not a return
	# value; awaiting a multi-argument signal yields the arguments as an array.
	var completed: Array = await http.request_completed
	http.queue_free()
	_seat_requesting = false

	var code := int(completed[0])
	var response_code := int(completed[1])
	var response_body := (completed[3] as PackedByteArray).get_string_from_utf8()
	if code != OK or response_code != 200:
		push_warning("CYGClient: matchmake failed (%d/%d): %s" % [code, response_code, response_body])
		seat_failed.emit("matchmake %d: %s" % [response_code, response_body])
		return

	var seat := _parse_seat(response_body)
	_seat = seat
	seat_ready.emit(seat)
	connect_socket(settings.game_socket_url(
		seat.process_id, seat.room_id, seat.session_id, seat.reconnection_token, seat.game_port
	), seat.reconnection_token)


## Node the transient `HTTPRequest` is attached to; `CYGNetwork` assigns itself.
## Godot refuses `add_child()` while a parent is still setting up children, so a
## host that is already in the tree is required.
var host: Node = null


## Overridable attachment point for the transient HTTPRequest node.
func get_parent_of_requested_node() -> Node:
	if host != null and host.is_inside_tree():
		return host
	var tree := Engine.get_main_loop() as SceneTree
	return tree.root if tree != null else null


func _parse_seat(json_text: String) -> CYGTypes.Seat:
	var seat := CYGTypes.Seat.new()
	var parsed := JSON.parse_string(json_text)
	if typeof(parsed) != TYPE_DICTIONARY:
		push_warning("CYGClient: matchmake reply was not a JSON object: " + json_text)
		return seat
	seat.name = str(parsed.get("name", ""))
	seat.session_id = str(parsed.get("sessionId", ""))
	seat.room_id = str(parsed.get("roomId", ""))
	seat.process_id = str(parsed.get("processId", ""))
	seat.reconnection_token = str(parsed.get("reconnectionToken", parsed.get("seatToken", "")))
	seat.game_port = int(parsed.get("gamePort", 0))
	return seat


## Opens the game socket directly; used after a seat is reserved, and by games
## that build their own url.
func connect_socket(url: String, auth_token: String = "") -> void:
	if url.is_empty():
		push_error("CYGClient: connect requires a non-empty url.")
		return
	close()
	_url = url
	_token = auth_token
	last_close_code = 0
	_outbound.clear()
	var err := _peer.connect_to_url(url)
	if err != OK:
		push_error("CYGClient: could not open the socket: %s" % error_string(err))
		socket_closed.emit("connect failed", 0)


func disconnect_from_room() -> void:
	if is_connected:
		send_wire(CYGOpcodes.WireLeaveRoom, PackedByteArray())
	close()


func close() -> void:
	is_connected = false
	_peer.close()
	_state = WebSocketPeer.STATE_CLOSED
	_outbound.clear()


## Test seam for reconnect logic: tears the socket down the way a yanked cable does,
## leaving the transition for `poll()` to notice. `close()` cannot stand in for a drop
## because it clears `is_connected` itself, so the closed branch would have nothing to
## report and `socket_closed` would never fire. `code` defaults to the SDK's own
## "you may try again" close code, so a caller dropping the link sees the same
## reconnect decision the room relay makes. No LeaveRoom wire frame is sent: the point
## is that the peer is already unreachable.
func simulate_drop(reason: String = "simulated-drop", code: int = CYGOpcodes.CloseMayTryReconnect) -> void:
	if _state == WebSocketPeer.STATE_CONNECTING or _state == WebSocketPeer.STATE_OPEN:
		_peer.close(code, reason)


## Call once per frame: drains the socket, flushes outbound and drives pings.
func poll() -> void:
	if _state == WebSocketPeer.STATE_CLOSED and _url.is_empty():
		return
	_peer.poll()
	_state = _peer.get_ready_state()
	match _state:
		WebSocketPeer.STATE_CONNECTING:
			pass
		WebSocketPeer.STATE_OPEN:
			if not is_connected:
				is_connected = true
				_last_ping_at = Time.get_ticks_msec() / 1000.0
				socket_opened.emit()
			_flush_outbound()
			_drain_inbound()
		WebSocketPeer.STATE_CLOSED:
			if is_connected:
				is_connected = false
				var code := _peer.get_close_code()
				last_close_code = code
				socket_closed.emit(_peer.get_close_reason(), code)
			_url = ""

	if is_connected:
		var now := Time.get_ticks_msec() / 1000.0
		if now - _last_ping_at >= PING_INTERVAL_SECONDS:
			_last_ping_at = now
			_ping_seq = now * 1000.0
			if on_ping_sent.is_valid():
				on_ping_sent.call()
			_ping_writer.reset()
			_ping_writer.write_uint32_be(int(_ping_seq) & 0xFFFFFFFF)
			send_frame(CYGOpcodes.Ping, _ping_writer.to_bytes())


func _drain_inbound() -> void:
	var available := _peer.get_available_packet_count()
	for i in range(available):
		var message := _peer.get_packet()
		_deframe(message)


func _flush_outbound() -> void:
	if _outbound.is_empty():
		return
	# One WebSocket message per canonical frame: Godot concatenates queued
	# packets, so several put_packet calls before the next poll would merge.
	while not _outbound.is_empty():
		var message := _outbound.pop_front()
		if _peer.put_packet(message) != OK:
			_outbound.clear()
			return


## Frames a payload as a canonical packet and hands it to the socket.
func send_frame(op: int, payload: PackedByteArray) -> bool:
	if not is_connected:
		push_warning("CYGClient: dropped a %s packet: the socket is not open." % op)
		return false
	var count := payload.size()
	if count > 0xFFFF:
		push_error("CYGClient: payload of %d bytes cannot fit the 16-bit length header." % count)
		return false

	var framed := CYGBitPacker.new()
	framed.write_uint16_be(count)
	framed.write_byte(op)
	framed.write_raw(payload)

	var message := CYGBitPacker.new()
	if CYGOpcodes.is_tunneled(op):
		message.write_byte(CYGOpcodes.to_wire(op))
		message.write_byte(CYGOpcodes.TUNNEL_MAGIC)
		message.write_raw(framed.to_bytes())
	else:
		message.write_byte(CYGOpcodes.to_wire(op))
		message.write_raw(payload)

	var bytes := message.to_bytes()
	if bytes.size() > MAX_WIRE_MESSAGE_SIZE:
		push_error("CYGClient: packet too large for one websocket message: %d bytes." % bytes.size())
		return false
	_outbound.append(bytes)
	return true


func send_wire(code: int, body: PackedByteArray) -> bool:
	if not is_connected:
		push_warning("CYGClient: dropped wire code %d: the socket is not open." % code)
		return false
	var message := CYGBitPacker.new()
	message.write_byte(code)
	message.write_raw(body)
	_outbound.append(message.to_bytes())
	return true


func _deframe(message: PackedByteArray) -> void:
	var length := message.size()
	if length == 0:
		return

	var leading := message[0] & CYGOpcodes.CODE_MASK

	# Room payloads ride ROOM_DATA_BYTES as [17]['C'][canonical frames...].
	if leading == CYGOpcodes.WireRoomDataBytes and length > 2 \
			and message[1] == CYGOpcodes.TUNNEL_MAGIC:
		var frames := _count_canonical_frames(message, 2, length - 2)
		if frames > 0:
			var position := 2
			for i in range(frames):
				var payload_length := _be16(message, position)
				frame_received.emit(message.slice(position, position + CYGOpcodes.HEADER_SIZE + payload_length))
				position += CYGOpcodes.HEADER_SIZE + payload_length
			return

	if leading == CYGOpcodes.WireError:
		server_error.emit(_extract_error_text(message.slice(1)))
		return

	var canonical := CYGOpcodes.from_wire(leading)
	if canonical >= 0:
		_emit_normalized(canonical, message.slice(1))
		return

	var counted := _count_canonical_frames(message, 0, length)
	if counted > 0:
		var position := 0
		for i in range(counted):
			var payload_length := _be16(message, position)
			frame_received.emit(message.slice(position, position + CYGOpcodes.HEADER_SIZE + payload_length))
			position += CYGOpcodes.HEADER_SIZE + payload_length
		return

	push_warning("CYGClient: could not deframe a %d-byte message (leading byte %d)." % [length, leading])


## The engine sends rejections as JSON, sometimes msgpack-framed; surface the
## printable part either way.
func _extract_error_text(payload: PackedByteArray) -> String:
	var text := payload.get_string_from_utf8()
	var brace := text.find("{")
	return text.substr(brace) if brace >= 0 else text


func _emit_normalized(op: int, payload: PackedByteArray) -> void:
	var framed := CYGBitPacker.new()
	framed.write_uint16_be(payload.size())
	framed.write_byte(op)
	framed.write_raw(payload)
	frame_received.emit(framed.to_bytes())


## Reads a big-endian u16 at `at`, the length prefix of a canonical frame.
func _be16(data: PackedByteArray, at: int) -> int:
	if at + 1 >= data.size():
		return 0
	return (int(data[at]) << 8) | int(data[at + 1])


func _count_canonical_frames(data: PackedByteArray, start: int, length: int) -> int:
	var frames := 0
	var position := start
	var end := start + length
	while position < end:
		if end - position < CYGOpcodes.HEADER_SIZE:
			return 0
		var payload_length := _be16(data, position)
		var total := CYGOpcodes.HEADER_SIZE + payload_length
		if position + total > end:
			return 0
		if not CYGOpcodes.is_canonical_op(data[position + 2]):
			return 0
		position += total
		frames += 1
	return frames
