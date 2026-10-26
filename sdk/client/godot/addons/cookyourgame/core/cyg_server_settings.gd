class_name CYGServerSettings
extends Resource
## Resource holding the endpoints and room defaults of a CYG client build.
##
## Mirrors `CYGServerSettings` in the Unity SDK: the matchmake url is built
## from the http endpoint, and the game socket url mirrors it onto ws/wss with
## the seat's process id, room id and session query.

const DEFAULT_ROOM_NAME := "cyg_room"
const DEFAULT_MATCHMAKE_METHOD := "joinOrCreate"

## Query flag the engine reads to route a room into the CYG relay (the same
## path the Unity SDK takes): tunneled RPCs, spawns and view patches are
## relayed to peers without any room logic.
const RELAY_QUERY := "cyg"

const DIGITS := "0123456789"

@export var server_http_url: String = "http://localhost:7777"
@export var server_ws_path: String = ""
@export var room_name: String = DEFAULT_ROOM_NAME
@export var max_players: int = 8
@export var protocol_version: int = 1
@export var matchmake_method: String = DEFAULT_MATCHMAKE_METHOD


func matchmake_url(method: String, room: String) -> String:
	var base := _trim_slashes(server_http_url)
	if base.is_empty():
		push_error("CYGServerSettings: server_http_url is empty.")
		return ""
	var resolved := method if not method.is_empty() else matchmake_method
	return base + "/matchmake/" + resolved.uri_encode() \
		+ "/" + normalize_room_name(room).uri_encode()


func game_socket_url(process_id: String, room_id: String, session_id: String, seat_token: String, game_port: int = 0) -> String:
	var parsed := _parse_http(server_http_url)
	if parsed.is_empty():
		push_error("CYGServerSettings: server_http_url is not a valid http url: " + server_http_url)
		return ""

	var scheme := "wss" if parsed["scheme"] == "https" else "ws"
	var port := game_port if game_port > 0 else int(parsed["port"])
	var room := (room_id if not room_id.is_empty() else normalize_room_name(room_name)).uri_encode()
	var process := (process_id if not process_id.is_empty() else "cyg").uri_encode()

	var url := "%s://%s" % [scheme, parsed["host"]]
	if port > 0 and not (port == 443 and scheme == "wss") and not (port == 80 and scheme == "ws"):
		url += ":%d" % port
	var path := _trim_slashes(server_ws_path)
	if not path.is_empty():
		url += "/" + path
	url += "/" + process + "/" + room

	var query := "sessionId=" + session_id.uri_encode()
	query += "&" + RELAY_QUERY
	if not seat_token.is_empty():
		query += "&reconnectionToken=" + seat_token.uri_encode()
	return url + "?" + query


## Clamps a room name to the server rule: 1-64 letters, digits, '-' or '_'.
static func normalize_room_name(room: String) -> String:
	if room.is_empty():
		return DEFAULT_ROOM_NAME
	var out := ""
	for i in range(room.length()):
		if out.length() >= 64:
			break
		var c := room[i]
		var is_letter := c != c.to_lower() or c != c.to_upper()
		if c == "-" or c == "_" or is_letter or DIGITS.find(c) >= 0:
			out += c
	return out if not out.is_empty() else DEFAULT_ROOM_NAME


## Splits an http(s) url into scheme, host and port (defaults to 80/443).
static func _parse_http(url: String) -> Dictionary:
	var scheme := ""
	if url.begins_with("https://"):
		scheme = "https"
	elif url.begins_with("http://"):
		scheme = "http"
	else:
		return {}
	var rest := url.trim_prefix(scheme + "://")
	var authority := rest.split("/", false)[0]
	var host := authority
	var port := 0
	var colon := authority.rfind(":")
	if colon >= 0:
		host = authority.substr(0, colon)
		port = int(authority.substr(colon + 1))
	if host.is_empty():
		return {}
	if port == 0:
		port = 443 if scheme == "https" else 80
	return {"scheme": scheme, "host": host, "port": port}


static func _trim_slashes(value: String) -> String:
	return value.trim_prefix("/").trim_suffix("/")
