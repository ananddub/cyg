class_name OpenOxideClient
extends Node
## Matchmaking client of the OpenOxide SDK, ported from `OpenOxideClient` in
## `sdk/web/openoxide.js`.
##
## Reserves a seat over HTTP (`POST /matchmake/{method}/{room}`), then opens the
## room WebSocket on the seat's game port. Rooms created here are polled by this
## node every frame, so the game only has to `await client.join_or_create(...)`
## and then use the returned `OpenOxideRoom`.
##
## ```gdscript
## var client := OpenOxideClient.new("http://127.0.0.1:8080")
## add_child(client)
## var room: OpenOxideRoom = await client.join_or_create("arena")
## room.use_schema(...)
## ```
##
## `reserve_seat()` must run from inside the scene tree: Godot's `HTTPRequest`
## is a Node and needs a tree to deliver `request_completed`.

signal room_joined(room: OpenOxideRoom)
signal matchmake_failed(method: String, room_name: String, status: int, detail: String)
signal rooms_fetched(rooms: Array)
signal rooms_failed(status: int, detail: String)

const VERSION := "1.5.0"

var endpoint: String
var options: Dictionary
var token: Variant = null
var last_seat: Dictionary = {}
var last_error := ""
var last_status := 0

## Seconds to wait for the room socket before a join is reported as failed.
var connect_timeout_seconds := 10.0

var _auth_callback: Callable = Callable()
var _auth_token: Variant = null
var _rooms: Array = []
var _api: OpenOxideApi = null


func _init(client_endpoint: String = "", client_options: Dictionary = {}) -> void:
	endpoint = client_endpoint
	options = client_options
	_auth_token = client_options.get("token")
	token = _auth_token


func _process(_delta: float) -> void:
	for room in _rooms.duplicate():
		if room != null:
			room.poll()


## Register an auth hook. The callable may return a token string or `{ token }`.
func on_auth(callback: Callable) -> OpenOxideClient:
	_auth_callback = callback
	return self


## Set the fallback bearer token (an `on_auth` hook still wins).
func set_token(value: Variant) -> OpenOxideClient:
	_auth_token = value
	token = value
	return self


## Current token: the `on_auth` hook's value when registered, else the stored one.
func resolve_token() -> Variant:
	var auth_token: Variant = _auth_token
	if _auth_callback.is_valid():
		var value: Variant = _auth_callback.call()
		if value is String:
			auth_token = value
		elif value is Dictionary and (value as Dictionary).has("token"):
			auth_token = (value as Dictionary)["token"]
	return auth_token


## REST surface shared with this client's endpoint and token: every
## `/v2/*` account service, matchmake, room listing, status and signaling URL.
func api() -> OpenOxideApi:
	if _api == null:
		_api = OpenOxideApi.new(http_base())
		_api.name = "OpenOxideApi"
		_api.bind_client(self)
		add_child(_api)
	else:
		_api.endpoint = http_base()
	return _api


## Shortcut for `api().request(...)`.
func request(http_method: String, path: String, body: Variant = null,
		query: Dictionary = {}) -> Dictionary:
	return await api().request(http_method, path, body, query)


func http_base() -> String:
	var value := endpoint.strip_edges().trim_suffix("/")
	while value.ends_with("/"):
		value = value.substr(0, value.length() - 1)
	if value.begins_with("ws://"):
		return "http://" + value.substr(5)
	if value.begins_with("wss://"):
		return "https://" + value.substr(6)
	if not (value.begins_with("http://") or value.begins_with("https://")):
		return "http://" + value
	return value


func _http_base() -> String:
	return http_base()


func ws_base(seat: Dictionary = {}) -> String:
	var base := _parse_base(http_base())
	var secure: bool = base["scheme"] == "https"
	var proto := "wss" if secure else "ws"
	var default_port := 443 if secure else 80
	var port := default_port
	if not String(base["port"]).is_empty():
		port = int(base["port"])
	var seat_port := int(float(seat.get("gamePort", 0)))
	if seat_port > 0:
		port = seat_port
	var path: String = base["path"]
	if path == "/":
		path = ""
	elif path.ends_with("/"):
		path = path.substr(0, path.length() - 1)
	return "%s://%s:%d%s" % [proto, base["host"], port, path]


func _ws_base(seat: Dictionary = {}) -> String:
	return ws_base(seat)


func auth_body(auth_options: Dictionary = {}) -> Dictionary:
	var body := auth_options.duplicate(true)
	var auth_token := resolve_token()
	if auth_token != null and not String(auth_token).is_empty():
		body["token"] = auth_token
	return { "body": body, "token": auth_token }


func _auth_body(auth_options: Dictionary = {}) -> Dictionary:
	return auth_body(auth_options)


## Reserve a seat. Returns the seat dictionary, or `{}` on failure (see
## `last_error` / `last_status`, and the `matchmake_failed` signal).
func reserve_seat(method: String, room_name: String, seat_options: Dictionary = {}) -> Dictionary:
	last_error = ""
	last_status = 0
	var auth := auth_body(seat_options)
	var auth_token: Variant = auth["token"]
	var url := "%s/matchmake/%s/%s" % [
		http_base(), method.uri_encode(), room_name.uri_encode()
	]
	var headers := PackedStringArray([
		"Content-Type: application/json",
		"Accept: application/json",
	])
	if auth_token != null and not String(auth_token).is_empty():
		headers.append("Authorization: Bearer %s" % auth_token)
	var response: Dictionary = await _http_request(
		HTTPClient.METHOD_POST, url, headers, JSON.stringify(auth["body"]))
	if not bool(response.get("ok", false)):
		var text := String(response.get("text", ""))
		var detail := text
		var parsed: Variant = JSON.parse_string(text)
		if parsed is Dictionary:
			var dict: Dictionary = parsed
			if dict.has("error") and not String(dict["error"]).is_empty():
				detail = String(dict["error"])
			elif dict.has("message") and not String(dict["message"]).is_empty():
				detail = String(dict["message"])
		last_status = int(response.get("status", 0))
		last_error = "[OpenOxide SDK] matchmake %s/%s failed (%d): %s" % [
			method, room_name, last_status, detail
		]
		push_warning(last_error)
		matchmake_failed.emit(method, room_name, last_status, detail)
		return {}
	var text := String(response.get("text", ""))
	var seat: Variant = JSON.parse_string(text)
	if not (seat is Dictionary):
		last_error = "[OpenOxide SDK] matchmake returned invalid JSON: %s" % text.substr(0, 200)
		push_warning(last_error)
		matchmake_failed.emit(method, room_name, int(response.get("status", 0)), last_error)
		return {}
	var seat_dict: Dictionary = seat
	if not seat_dict.has("sessionId") or String(seat_dict["sessionId"]).is_empty():
		last_error = "[OpenOxide SDK] matchmake reply carried no sessionId"
		push_warning(last_error)
		matchmake_failed.emit(method, room_name, int(response.get("status", 0)), last_error)
		return {}
	last_seat = seat_dict
	return seat_dict


func _reserve(method: String, room_name: String, seat_options: Dictionary = {}) -> Dictionary:
	return await reserve_seat(method, room_name, seat_options)


func _connect_seat(seat: Dictionary, room_name: String, room_options: Dictionary = {}) -> Variant:
	var merged := room_options.duplicate(true)
	merged["sessionId"] = seat.get("sessionId")
	merged["reconnectionToken"] = seat.get("reconnectionToken", seat.get("seatToken"))
	merged["client"] = self
	var room := OpenOxideRoom.new(ws_base(seat), room_name, merged)
	if not room.connect_room():
		last_error = "[OpenOxide SDK] room socket could not be opened"
		push_warning(last_error)
		return null
	_rooms.append(room)
	# Weak captures: a Callable bound to `room` and stored on `room` would make
	# the RefCounted room hold itself alive.
	var weak_room := weakref(room)
	var weak_self := weakref(self)
	room.closed.connect(func(code: int, reason: String) -> void:
		var owner: Variant = weak_self.get_ref()
		var closed_room: Variant = weak_room.get_ref()
		if owner != null and closed_room != null:
			owner._on_room_closed(code, reason, closed_room))
	room.reconnect_failed.connect(func() -> void:
		var owner: Variant = weak_self.get_ref()
		var failed_room: Variant = weak_room.get_ref()
		if owner != null and failed_room != null:
			owner._on_room_reconnect_failed(failed_room))
	var opened: bool = await room.wait_opened(connect_timeout_seconds)
	if not opened:
		last_error = "[OpenOxide SDK] room socket did not open in time"
		push_warning(last_error)
		room.leave(false)
		_rooms.erase(room)
		return null
	room_joined.emit(room)
	return room


## Colyseus-style matchmaking: the server mints a seat, the room opens the
## WebSocket on the game port with that sessionId.
func join_or_create(room_name: String, room_options: Dictionary = {}) -> Variant:
	var seat := await reserve_seat("joinOrCreate", room_name, room_options)
	if seat.is_empty():
		return null
	return await _connect_seat(seat, room_name, room_options)


func join(room_name: String, room_options: Dictionary = {}) -> Variant:
	var seat := await reserve_seat("join", room_name, room_options)
	if seat.is_empty():
		return null
	return await _connect_seat(seat, room_name, room_options)


func create(room_name: String, room_options: Dictionary = {}) -> Variant:
	var seat := await reserve_seat("create", room_name, room_options)
	if seat.is_empty():
		return null
	return await _connect_seat(seat, room_name, room_options)


func join_by_id(room_id: String, room_options: Dictionary = {}) -> Variant:
	var seat := await reserve_seat("joinById", room_id, room_options)
	if seat.is_empty():
		return null
	return await _connect_seat(seat, room_id, room_options)


func spectate(room_name: String, room_options: Dictionary = {}) -> Variant:
	var seat := await reserve_seat("spectate", room_name, room_options)
	if seat.is_empty():
		return null
	var spectator_options := room_options.duplicate(true)
	spectator_options["spectator"] = true
	return await _connect_seat(seat, room_name, spectator_options)


func join_spectator(room_name: String, room_options: Dictionary = {}) -> Variant:
	return await spectate(room_name, room_options)


## Resume a seat after a reload or a dropped socket.
func reconnect_room(reconnection_token: Variant, room_name: String,
		room_options: Dictionary = {}) -> Variant:
	var resume_options := room_options.duplicate(true)
	resume_options["reconnectionToken"] = reconnection_token
	var seat := await reserve_seat("reconnect", room_name, resume_options)
	if seat.is_empty():
		return null
	return await _connect_seat(seat, room_name, room_options)


## List joinable rooms, optionally filtered by room name or id.
func get_available_rooms(room_name: String = "") -> Array:
	var url := "%s/api/rooms" % http_base()
	var headers := PackedStringArray(["Accept: application/json"])
	var response: Dictionary = await _http_request(HTTPClient.METHOD_GET, url, headers, "")
	if not bool(response.get("ok", false)):
		last_status = int(response.get("status", 0))
		last_error = "[OpenOxide SDK] room list failed (%d)" % last_status
		push_warning(last_error)
		rooms_failed.emit(last_status, String(response.get("text", "")))
		return []
	var payload: Variant = JSON.parse_string(String(response.get("text", "")))
	var raw_rooms: Array = []
	if payload is Array:
		raw_rooms = payload
	elif payload is Dictionary and (payload as Dictionary).get("rooms") is Array:
		raw_rooms = (payload as Dictionary)["rooms"]
	var out: Array = []
	for entry in raw_rooms:
		if not (entry is Dictionary):
			continue
		var room: Dictionary = entry
		var name := String(room.get("name", ""))
		var room_id := String(room.get("roomId", name))
		if not room_name.is_empty() and name != room_name and room_id != room_name:
			continue
		var clients: Variant = room.get("clients")
		if clients == null:
			clients = room.get("players", 0)
		var normalized: Dictionary = room.duplicate(true)
		normalized["roomId"] = room_id
		normalized["name"] = name
		normalized["clients"] = clients
		normalized["maxClients"] = room.get("maxClients", 0)
		normalized["spectators"] = room.get("spectators", 0)
		normalized["metadata"] = room.get("metadata")
		out.append(normalized)
	rooms_fetched.emit(out)
	return out


## Drop a room from this client's per-frame polling list without closing it.
func release_room(room: OpenOxideRoom) -> void:
	_rooms.erase(room)


static func load_replay(json_or_data: Variant) -> OpenOxideReplayPlayer:
	var data: Dictionary = {}
	if json_or_data is String:
		var parsed: Variant = JSON.parse_string(json_or_data)
		data = parsed if parsed is Dictionary else {}
	elif json_or_data is Dictionary:
		data = json_or_data
	return OpenOxideReplayPlayer.new(data)


func _on_room_closed(code: int, _reason: String, room: OpenOxideRoom) -> void:
	if code == 1000 or code == 1001 or code == 4001 or code == 4002 or code == 4003:
		_rooms.erase(room)


func _on_room_reconnect_failed(room: OpenOxideRoom) -> void:
	_rooms.erase(room)


func _parse_base(base: String) -> Dictionary:
	var scheme := "http"
	var rest := base
	var scheme_end := rest.find("://")
	if scheme_end >= 0:
		scheme = rest.substr(0, scheme_end).to_lower()
		rest = rest.substr(scheme_end + 3)
	var slash := rest.find("/")
	var authority := rest if slash < 0 else rest.substr(0, slash)
	var path := "/" if slash < 0 else rest.substr(slash)
	var host := authority
	var port := ""
	if authority.begins_with("["):
		var close := authority.find("]")
		if close >= 0:
			host = authority.substr(0, close + 1)
			if authority.length() > close + 1 and authority[close + 1] == ":":
				port = authority.substr(close + 2)
	else:
		var colon := authority.rfind(":")
		if colon >= 0:
			host = authority.substr(0, colon)
			port = authority.substr(colon + 1)
	return { "scheme": scheme, "host": host, "port": port, "path": path }


func _http_request(method: int, url: String, headers: PackedStringArray,
		body: String) -> Dictionary:
	if not is_inside_tree():
		return {
			"ok": false, "status": 0, "text": "",
			"error": "OpenOxideClient must be inside the scene tree for HTTP matchmaking",
		}
	var request := HTTPRequest.new()
	request.timeout = 15.0
	add_child(request)
	var err := request.request(url, headers, method, body)
	if err != OK:
		request.queue_free()
		return { "ok": false, "status": 0, "text": "", "error": error_string(err) }
	var result: Array = await request.request_completed
	request.queue_free()
	var status := int(result[1])
	var payload: PackedByteArray = result[3] if result.size() > 3 else PackedByteArray()
	var success: bool = int(result[0]) == HTTPRequest.RESULT_SUCCESS and status >= 200 and status < 300
	return {
		"ok": success,
		"status": status,
		"text": payload.get_string_from_utf8(),
		"error": "" if success else String(result[0]),
	}
