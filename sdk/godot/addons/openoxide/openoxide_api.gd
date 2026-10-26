class_name OpenOxideApi
extends Node
## Player-facing REST surface of the OpenOxide server: every `/v2/*` route the
## engine-api registers, plus `/api/matchmake`, `/status`, `/api/rooms`, the
## health probes and the `/ws/signal` upgrade URL.
##
## The room socket (`OpenOxideRoom`) carries gameplay; this class carries the
## account and meta services a game needs around it: auth/session, account and
## wallet, user lookup, friends, groups, storage, inventory, leaderboards,
## tournaments, channels, notifications, IAP, match browsing.
##
## ```gdscript
## var api := OpenOxideApi.new("http://127.0.0.1:8080")
## add_child(api)
## var session := await api.authenticate_device({"id": OS.get_unique_id()})
## var account := await api.get_account()
## ```
##
## Every call returns `{ ok, status, data, text, error }`: `data` is the decoded
## JSON (or `null` when the body is not JSON), `error` is the server's
## `{"error":{"message":…}}` text on failure. Requests carry the bound client's
## token, resolved per call so `on_auth` hooks keep working.

signal response_received(method: String, path: String, status: int, data: Variant)
signal request_failed(method: String, path: String, status: int, detail: String)

const VERSION := "1.5.0"

var endpoint := ""
## Fallback bearer token. A bound `OpenOxideClient`'s token wins.
var token: Variant = null
var refresh_token: Variant = null
var timeout_seconds := 15.0
## Extra headers appended to every request.
var extra_headers := PackedStringArray()

var _client_ref: WeakRef = null


func _init(api_endpoint: String = "", api_token: Variant = null) -> void:
	endpoint = api_endpoint
	token = api_token


## Reuse another node's token resolver (any node with `resolve_token()`).
func bind_client(client: Node) -> OpenOxideApi:
	_client_ref = weakref(client)
	return self


func set_token(value: Variant) -> OpenOxideApi:
	token = value
	var client: Variant = client_node()
	if client != null and client.has_method("set_token"):
		client.set_token(value)
	return self


func client_node() -> Variant:
	return _client_ref.get_ref() if _client_ref != null else null


func resolve_token() -> Variant:
	var client: Variant = client_node()
	if client != null and client.has_method("resolve_token"):
		var value: Variant = client.resolve_token()
		if value != null and not String(value).is_empty():
			return value
	return token


func http_base() -> String:
	var value := endpoint.strip_edges()
	while value.ends_with("/"):
		value = value.substr(0, value.length() - 1)
	if value.begins_with("ws://"):
		return "http://" + value.substr(5)
	if value.begins_with("wss://"):
		return "https://" + value.substr(6)
	if not (value.begins_with("http://") or value.begins_with("https://")):
		return "http://" + value
	return value


func ws_base() -> String:
	var base := http_base()
	if base.begins_with("https://"):
		return "wss://" + base.substr(8)
	if base.begins_with("http://"):
		return "ws://" + base.substr(7)
	return base


func build_url(path: String, query: Dictionary = {}) -> String:
	var suffix := path
	if suffix.is_empty():
		suffix = "/"
	elif not suffix.begins_with("/"):
		suffix = "/" + suffix
	var url := http_base() + suffix
	var encoded := encode_query(query)
	if not encoded.is_empty():
		url += "?" + encoded
	return url


## `?ids=a&ids=b&limit=10`; arrays repeat the key, `null` is omitted.
func encode_query(query: Dictionary) -> String:
	var parts := PackedStringArray()
	var keys := query.keys()
	keys.sort()
	for key in keys:
		var value: Variant = query[key]
		if value == null:
			continue
		var name := _segment(key)
		if value is Array:
			for item in value:
				if item != null:
					parts.append("%s=%s" % [name, _query_value(item)])
		else:
			parts.append("%s=%s" % [name, _query_value(value)])
	return "&".join(parts)


func signal_url(seat: String = "") -> String:
	var url := ws_base() + "/ws/signal"
	if not seat.is_empty():
		url += "?seat=" + _segment(seat)
	return url


## Escape hatch for any route: `await api.request("GET", "/v2/account")`.
func request(http_method: String, path: String, body: Variant = null,
		query: Dictionary = {}) -> Dictionary:
	var method := _method_id(http_method)
	if method < 0:
		return _failure(http_method.to_upper(), path, 0,
			"Unsupported HTTP method: %s" % http_method)
	if http_base() == "http://":
		return _failure(http_method.to_upper(), path, 0, "OpenOxideApi endpoint is empty")
	var has_body := body != null
	var body_text := ""
	if has_body:
		body_text = body if body is String else JSON.stringify(body)
	var headers := PackedStringArray(["Accept: application/json"])
	if has_body:
		headers.append("Content-Type: application/json")
	var auth_token := resolve_token()
	if auth_token != null and not String(auth_token).is_empty():
		headers.append("Authorization: Bearer %s" % auth_token)
	headers.append_array(extra_headers)
	var raw: Dictionary = await _send(method, build_url(path, query), headers, body_text)
	var text := String(raw.get("text", ""))
	var data: Variant = null
	if not text.is_empty():
		var parsed: Variant = JSON.parse_string(text)
		if parsed != null or text.strip_edges() == "null":
			data = parsed
	var ok := bool(raw.get("ok", false))
	var detail := ""
	if not ok:
		detail = _error_detail(data, text)
		var transport := String(raw.get("error", ""))
		if detail.is_empty():
			detail = transport
	var result := {
		"ok": ok,
		"status": int(raw.get("status", 0)),
		"data": data,
		"text": text,
		"error": detail,
	}
	if ok:
		response_received.emit(http_method.to_upper(), path, int(result["status"]), data)
	else:
		request_failed.emit(http_method.to_upper(), path, int(result["status"]), detail)
	return result


# --- Auth ---------------------------------------------------------------------

func authenticate(body: Dictionary, query: Dictionary = {}) -> Dictionary:
	return await _auth_post("/v2/account/authenticate", body, query)


func authenticate_apple(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/apple", body)


func authenticate_custom(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/custom", body)


func authenticate_device(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/device", body)


func authenticate_email(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/email", body)


func authenticate_facebook(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/facebook", body)


func authenticate_facebook_instant_game(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/facebookinstantgame", body)


func authenticate_game_center(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/gamecenter", body)


func authenticate_google(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/google", body)


func authenticate_steam(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/authenticate/steam", body)


## `body` is `{ token, vars }`; the response's `token`/`refresh_token` are stored.
func refresh_session(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/session/refresh", body)


## `body` is `{ token, refresh_token }`; clears the local token on success.
func logout(body: Dictionary) -> Dictionary:
	var result := await _post("/v2/session/logout", body)
	if bool(result.get("ok", false)):
		set_token(null)
		refresh_token = null
	return result


# --- Account linking ----------------------------------------------------------

func link(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link", body)


func link_apple(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/apple", body)


func link_custom(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/custom", body)


func link_device(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/device", body)


func link_email(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/email", body)


func link_facebook(body: Dictionary, sync: Variant = null) -> Dictionary:
	return await _auth_post("/v2/account/link/facebook", body, _sync_query(sync))


func link_facebook_instant_game(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/facebookinstantgame", body)


func link_game_center(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/gamecenter", body)


func link_google(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/google", body)


func link_steam(body: Dictionary) -> Dictionary:
	return await _auth_post("/v2/account/link/steam", body)


func unlink(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink", body)


func unlink_apple(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/apple", body)


func unlink_custom(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/custom", body)


func unlink_device(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/device", body)


func unlink_email(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/email", body)


func unlink_facebook(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/facebook", body)


func unlink_facebook_instant_game(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/facebookinstantgame", body)


func unlink_game_center(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/gamecenter", body)


func unlink_google(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/google", body)


func unlink_steam(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/unlink/steam", body)


# --- Account & wallet ---------------------------------------------------------

func get_account() -> Dictionary:
	return await _get_request("/v2/account")


func update_account(body: Dictionary, query: Dictionary = {}) -> Dictionary:
	return await _put("/v2/account", body, query)


func delete_account() -> Dictionary:
	return await _delete("/v2/account")


func get_wallet() -> Dictionary:
	return await _get_request("/v2/account/wallet")


func update_wallet(body: Dictionary) -> Dictionary:
	return await _post("/v2/account/wallet", body)


## Metadata merge only; balances move from the console.
func merge_wallet(body: Dictionary) -> Dictionary:
	return await _put("/v2/account/wallet", body)


# --- Users --------------------------------------------------------------------

func get_users(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/user", query)


func get_users_by_body(body: Dictionary) -> Dictionary:
	return await _post("/v2/user", body)


func get_user(user_id: String) -> Dictionary:
	return await _get_request("/v2/user/%s" % _segment(user_id))


# --- Friends ------------------------------------------------------------------

func list_friends(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/friend", query)


## `query` is `{ ids: [...], usernames: [...], metadata: "…" }`.
func add_friends(query: Dictionary) -> Dictionary:
	return await _post("/v2/friend", null, query)


func delete_friends(query: Dictionary) -> Dictionary:
	return await _delete("/v2/friend", null, query)


func block_friends(query: Dictionary) -> Dictionary:
	return await _post("/v2/friend/block", null, query)


func list_friends_of_friends(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/friend/friends", query)


func import_facebook_friends() -> Dictionary:
	return await _post("/v2/friend/facebook")


func import_steam_friends() -> Dictionary:
	return await _post("/v2/friend/steam")


# --- Groups -------------------------------------------------------------------

func list_groups(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/group", query)


func create_group(body: Dictionary) -> Dictionary:
	return await _post("/v2/group", body)


func get_group(group_id: String) -> Dictionary:
	return await _get_request("/v2/group/%s" % _segment(group_id))


func update_group(group_id: String, body: Dictionary) -> Dictionary:
	return await _put("/v2/group/%s" % _segment(group_id), body)


func delete_group(group_id: String) -> Dictionary:
	return await _delete("/v2/group/%s" % _segment(group_id))


func join_group(group_id: String) -> Dictionary:
	return await _post("/v2/group/%s/join" % _segment(group_id))


func leave_group(group_id: String) -> Dictionary:
	return await _post("/v2/group/%s/leave" % _segment(group_id))


func list_group_users(group_id: String, query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/group/%s/user" % _segment(group_id), query)


## The five member verbs take `?user_ids=a&user_ids=b`.
func add_group_users(group_id: String, query: Dictionary) -> Dictionary:
	return await _post("/v2/group/%s/add" % _segment(group_id), null, query)


func kick_group_users(group_id: String, query: Dictionary) -> Dictionary:
	return await _post("/v2/group/%s/kick" % _segment(group_id), null, query)


func ban_group_users(group_id: String, query: Dictionary) -> Dictionary:
	return await _post("/v2/group/%s/ban" % _segment(group_id), null, query)


func promote_group_users(group_id: String, query: Dictionary) -> Dictionary:
	return await _post("/v2/group/%s/promote" % _segment(group_id), null, query)


func demote_group_users(group_id: String, query: Dictionary) -> Dictionary:
	return await _post("/v2/group/%s/demote" % _segment(group_id), null, query)


func list_user_groups(user_id: String, query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/user/%s/group" % _segment(user_id), query)


# --- Tournaments --------------------------------------------------------------

func list_tournaments(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/tournament", query)


func list_tournament_records(tournament_id: String, query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/tournament/%s" % _segment(tournament_id), query)


func write_tournament_record(tournament_id: String, body: Variant) -> Dictionary:
	return await _post("/v2/tournament/%s" % _segment(tournament_id), body)


func replace_tournament_record(tournament_id: String, body: Variant) -> Dictionary:
	return await _put("/v2/tournament/%s" % _segment(tournament_id), body)


func delete_tournament_record(tournament_id: String) -> Dictionary:
	return await _delete("/v2/tournament/%s" % _segment(tournament_id))


func list_tournament_records_around_owner(tournament_id: String, owner_id: String,
		query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/tournament/%s/owner/%s" % [
		_segment(tournament_id), _segment(owner_id)
	], query)


func join_tournament(tournament_id: String) -> Dictionary:
	return await _post("/v2/tournament/%s/join" % _segment(tournament_id))


# --- Leaderboards -------------------------------------------------------------

func write_leaderboard_record(leaderboard_id: String, body: Variant) -> Dictionary:
	return await _post("/v2/leaderboard/%s" % _segment(leaderboard_id), body)


func list_leaderboard_records(leaderboard_id: String, query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/leaderboard/%s" % _segment(leaderboard_id), query)


func delete_leaderboard_record(leaderboard_id: String) -> Dictionary:
	return await _delete("/v2/leaderboard/%s" % _segment(leaderboard_id))


func list_leaderboard_records_around_owner(leaderboard_id: String, owner_id: String,
		query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/leaderboard/%s/owner/%s" % [
		_segment(leaderboard_id), _segment(owner_id)
	], query)


# --- Storage ------------------------------------------------------------------

## `{ objects: [{ collection, key, value, version, permission_read, permission_write }] }`
func write_storage(body: Dictionary) -> Dictionary:
	return await _put("/v2/storage", body)


## `{ object_ids: [{ collection, key, user_id, version }] }`
func read_storage(body: Dictionary) -> Dictionary:
	return await _post("/v2/storage", body)


func delete_storage(body: Dictionary) -> Dictionary:
	return await _put("/v2/storage/delete", body)


func list_storage_collection(collection: String, query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/storage/%s" % _segment(collection), query)


func list_user_storage(collection: String, user_id: String,
		query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/storage/%s/%s" % [
		_segment(collection), _segment(user_id)
	], query)


func read_storage_object(collection: String, key: String, user_id: String,
		query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/storage/%s/%s/%s" % [
		_segment(collection), _segment(key), _segment(user_id)
	], query)


# --- Storage indexes ----------------------------------------------------------

func create_storage_index(body: Dictionary) -> Dictionary:
	return await _post("/v2/storage/index", body)


func list_storage_indexes() -> Dictionary:
	return await _get_request("/v2/storage/index")


func delete_storage_index(name: String) -> Dictionary:
	return await _delete("/v2/storage/index/%s" % _segment(name))


func query_storage_index(name: String, body: Dictionary) -> Dictionary:
	return await _post("/v2/storage/index/%s/query" % _segment(name), body)


# --- Inventory ----------------------------------------------------------------

func list_inventory() -> Dictionary:
	return await _get_request("/v2/inventory")


func equip_item(body: Dictionary) -> Dictionary:
	return await _post("/v2/inventory/equip", body)


func consume_item(body: Dictionary) -> Dictionary:
	return await _post("/v2/inventory/consume", body)


func grant_item(body: Dictionary) -> Dictionary:
	return await _post("/v2/inventory/grant", body)


func define_item(body: Dictionary) -> Dictionary:
	return await _post("/v2/inventory/define", body)


# --- Channels -----------------------------------------------------------------

func list_channel_messages(channel_id: String, query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/channel/%s" % _segment(channel_id), query)


func send_channel_message(channel_id: String, body: Dictionary) -> Dictionary:
	return await _post("/v2/channel/%s/message" % _segment(channel_id), body)


func update_channel_message(channel_id: String, message_id: String,
		body: Dictionary) -> Dictionary:
	return await _put("/v2/channel/%s/message/%s" % [
		_segment(channel_id), _segment(message_id)
	], body)


func delete_channel_message(channel_id: String, message_id: String) -> Dictionary:
	return await _delete("/v2/channel/%s/message/%s" % [
		_segment(channel_id), _segment(message_id)
	])


# --- Notifications ------------------------------------------------------------

func list_notifications(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/notification", query)


## `{ ids: [...] }`
func delete_notifications(body: Dictionary) -> Dictionary:
	return await _delete("/v2/notification", body)


func delete_notification(notification_id: String) -> Dictionary:
	return await _delete("/v2/notification/%s" % _segment(notification_id))


# --- IAP ----------------------------------------------------------------------

## Legacy Apple receipt validation (`POST /v2/iap`).
func validate_apple_purchase(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap", body)


func purchase_apple(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/purchase/apple", body)


func purchase_google(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/purchase/google", body)


func purchase_huawei(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/purchase/huawei", body)


func purchase_facebook_instant(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/purchase/facebookinstant", body)


func purchase_samsung(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/purchase/samsung", body)


func list_purchases(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/iap/purchase", query)


func subscribe_apple(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/subscription/apple", body)


func subscribe_google(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/subscription/google", body)


func subscribe_huawei(body: Dictionary) -> Dictionary:
	return await _post("/v2/iap/subscription/huawei", body)


func list_subscriptions(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/iap/subscription", query)


func get_subscription(product_id: String) -> Dictionary:
	return await _get_request("/v2/iap/subscription/%s" % _segment(product_id))


# --- Match browsing -----------------------------------------------------------

func list_matches(query: Dictionary = {}) -> Dictionary:
	return await _get_request("/v2/match", query)


func matchmaker_stats() -> Dictionary:
	return await _get_request("/v2/matchmaker/stats")


# --- Matchmaking & server status ---------------------------------------------

## `POST /matchmake/{method}/{room}` — the short form `OpenOxideClient` uses.
func matchmake(method: String, room: String, body: Dictionary = {}) -> Dictionary:
	return await _post("/matchmake/%s/%s" % [_segment(method), _segment(room)], body)


## `POST /api/matchmake/{method}/{room}`.
func matchmake_api(method: String, room: String, body: Dictionary = {}) -> Dictionary:
	return await _post("/api/matchmake/%s/%s" % [_segment(method), _segment(room)], body)


func status() -> Dictionary:
	return await _get_request("/status")


func rooms() -> Dictionary:
	return await _get_request("/api/rooms")


func health() -> Dictionary:
	return await _get_request("/health")


func auto_health() -> Dictionary:
	return await _get_request("/auto/health")


# --- Internals ----------------------------------------------------------------

func _get_request(path: String, query: Dictionary = {}) -> Dictionary:
	return await request("GET", path, null, query)


func _post(path: String, body: Variant = null, query: Dictionary = {}) -> Dictionary:
	return await request("POST", path, body, query)


func _put(path: String, body: Variant = null, query: Dictionary = {}) -> Dictionary:
	return await request("PUT", path, body, query)


func _delete(path: String, body: Variant = null, query: Dictionary = {}) -> Dictionary:
	return await request("DELETE", path, body, query)


func _auth_post(path: String, body: Variant = null, query: Dictionary = {}) -> Dictionary:
	var result := await _post(path, body, query)
	if bool(result.get("ok", false)):
		_capture_session(result.get("data"))
	return result


func _capture_session(data: Variant) -> void:
	if not (data is Dictionary):
		return
	var source: Dictionary = data
	if source.get("session") is Dictionary:
		source = source["session"]
	if source.has("token") and not String(source["token"]).is_empty():
		set_token(source["token"])
	if source.has("refresh_token") and not String(source["refresh_token"]).is_empty():
		refresh_token = source["refresh_token"]


func _sync_query(sync: Variant) -> Dictionary:
	return {} if sync == null else { "sync": sync }


func _segment(value: Variant) -> String:
	return str(value).uri_encode()


func _query_value(value: Variant) -> String:
	if value is bool:
		return "true" if value else "false"
	if value is Dictionary or value is Array:
		return JSON.stringify(value).uri_encode()
	return _segment(value)


func _method_id(http_method: String) -> int:
	match http_method.to_upper():
		"GET": return HTTPClient.METHOD_GET
		"POST": return HTTPClient.METHOD_POST
		"PUT": return HTTPClient.METHOD_PUT
		"DELETE": return HTTPClient.METHOD_DELETE
		"PATCH": return HTTPClient.METHOD_PATCH
		"HEAD": return HTTPClient.METHOD_HEAD
		"OPTIONS": return HTTPClient.METHOD_OPTIONS
	return -1


func _failure(http_method: String, path: String, status: int, detail: String) -> Dictionary:
	request_failed.emit(http_method, path, status, detail)
	return { "ok": false, "status": status, "data": null, "text": "", "error": detail }


func _send(method: int, url: String, headers: PackedStringArray, body: String) -> Dictionary:
	if not is_inside_tree():
		return {
			"ok": false, "status": 0, "text": "",
			"error": "OpenOxideApi must be inside the scene tree for HTTP requests",
		}
	var http := HTTPRequest.new()
	http.timeout = timeout_seconds
	add_child(http)
	var err := http.request(url, headers, method, body)
	if err != OK:
		http.queue_free()
		return { "ok": false, "status": 0, "text": "", "error": error_string(err) }
	var completed: Array = await http.request_completed
	http.queue_free()
	var result_id := int(completed[0])
	var code := int(completed[1])
	var payload: PackedByteArray = completed[3] if completed.size() > 3 else PackedByteArray()
	return {
		"ok": result_id == HTTPRequest.RESULT_SUCCESS and code >= 200 and code < 300,
		"status": code,
		"text": payload.get_string_from_utf8(),
		"error": "" if result_id == HTTPRequest.RESULT_SUCCESS else "HTTP transport error %d" % result_id,
	}


func _error_detail(data: Variant, text: String) -> String:
	if data is Dictionary:
		var envelope: Dictionary = data
		var error_value: Variant = envelope.get("error")
		if error_value is Dictionary:
			var message: Variant = (error_value as Dictionary).get("message")
			if message != null and not String(message).is_empty():
				return String(message)
		elif error_value is String and not String(error_value).is_empty():
			return error_value
		var top: Variant = envelope.get("message")
		if top != null and not String(top).is_empty():
			return String(top)
	if not text.is_empty():
		return text.substr(0, 512)
	return ""
