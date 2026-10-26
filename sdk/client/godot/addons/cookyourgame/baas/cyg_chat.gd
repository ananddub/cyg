class_name CYGChat
extends RefCounted
## Text channels over the room server's `/v2/channel` routes, with a local
## message cache a UI can bind to through signals.
##
## Parity: the Unity SDK (`sdk/unity/Runtime/BaaS/CYGChat.cs`) and the Unreal
## SDK (`sdk/unreal/CookYourGame/Source/CookYourGame/Public/BaaS/CYGBaasChat.h`)
## expose the same routes and cache semantics; this file is the GDScript port.
## The server's HTTP channel API is the contract — the routes in
## `crates/engine-api/src/controllers/channel.rs` and the shapes in
## `crates/engine-api/src/dto/channel.rs`.
##
## Channels are NOT addressed under `/v2/group`: group routes only manage
## membership (`join`/`leave`/`add`/…); a group's stream is still read and
## written through `/v2/channel/{channel_id}`. The stream's identity is
## flattened into each message by mode: `room_name` (room stream), `group_id`
## (group stream) or `user_id_one`/`user_id_two` (direct messages) — exactly
## one of those families appears per message, and an absent member is an
## absent key.
##
## A channel id is the server's dotted four-part id
## (`<mode>.<subject>.<descriptor>.<label>`); use room_channel(),
## group_channel() or direct_channel() instead of passing a bare room name.
##
## Endpoints, exactly as the server declares them:
##
##  * `GET /v2/channel/{channel_id}` — a page of stored messages.
##    Query (`ListQuery`): `limit` (int64), `cursor`, `forward` (bool; the
##    server default `true` reads oldest-to-newest from the head, `false`
##    walks back from the newest). Replies `MessageListResponse`:
##
##        {"messages": [apiChannelMessage], "next_cursor"?, "prev_cursor"?,
##         "cacheable_cursor"?}
##
##  * `POST /v2/channel/{channel_id}/message` — send. Body (`SendRequest`):
##
##        {"content": <object or array>}
##
##    `content` is client JSON passed through unvalidated beyond "object or
##    array", so plain text is wrapped as {"text": …} here. Replies the stored
##    `apiChannelMessage`.
##
##  * `PUT /v2/channel/{channel_id}/message/{message_id}` — edit. Body
##    (`UpdateRequest`): {"content": <object or array>}. Replies the updated
##    `apiChannelMessage`.
##
##  * `DELETE /v2/channel/{channel_id}/message/{message_id}` — remove. Replies
##    the deleted `apiChannelMessage`.
##
## `apiChannelMessage` fields (`MessageResponse`): `channel_id`, `message_id`,
## `code` (int16), `sender_id`, `username`? (absent when empty), `content` (the
## client JSON re-serialized as a *string*), `create_time`/`update_time` (ISO
## strings), `persistent` (bool), and the mode-dependent stream identity named
## above.
##
## Transport is deliberately HTTP-only and swappable: live pushes (whatever
## replaces the realtime WebSocket handlers being removed from
## `controllers/realtime/handlers/`) can feed the same cache and signals by
## calling `ingest_message()` with an `apiChannelMessage` Dictionary or JSON
## string — no other method needs to change.
##
## Callback contract, every public method (identical to `CYGLeaderboard`'s):
##
##     callback(response: Dictionary)  # {is_success: bool, status_code: int,
##                                      #  text: String, json: Variant,
##                                      #  data: Variant, error: String}
##
## On success `data` holds the parsed shape; on any failure `is_success` is
## false and `data` is null. Ordinary non-2xx answers are not pushed as engine
## errors. The cache and the signals only move on success.
##
## Usage:
##
##     var chat := CYGChat.new()
##     chat.message_received.connect(func(message): add_line(message["content"]))
##     chat.messages_updated.connect(func(channel_id, messages): refresh(channel_id, messages))
##     chat.history(CYGChat.room_channel("lobby"), func(response):
##         if response["is_success"]:
##             refresh("lobby", chat.messages_for(CYGChat.room_channel("lobby"))))
##     chat.send(CYGChat.room_channel("lobby"), "gg", Callable())
##
## Independent GDScript port of this repo's Unity/Unreal BaaS layer onto the
## documented `/v2/` endpoints of `crates/engine-api`; no upstream source text
## is copied.

signal message_received(message: Dictionary)
signal messages_updated(channel_id: String, messages: Array)

const CACHE_LIMIT := 500
const _MAX_BODY_ERROR := 240

var _cache := {}  # channel_id -> Array[Dictionary], oldest first


## The dotted channel id of a room stream: `2..<room>`.
static func room_channel(room_name: String) -> String:
	return "2.." + room_name


## The dotted channel id of a group stream: `3.<group_id>..`.
static func group_channel(group_id: String) -> String:
	return "3." + group_id.to_lower() + ".."


## The canonical dotted channel id of a direct stream: `4.<min>.<max>.`.
static func direct_channel(user_id_one: String, user_id_two: String) -> String:
	var one := user_id_one.to_lower()
	var two := user_id_two.to_lower()
	if one > two:
		var swap := one
		one = two
		two = swap
	return "4.%s.%s." % [one, two]


## Stored history for `channel_id`, page knobs matching `ListQuery`: `limit`
## (0 omits it), `cursor`, `forward` (sent only when `false`, since the server
## already defaults to `true`). Merges the page into the cache and answers
## `data` = {"messages", "next_cursor", "prev_cursor", "cacheable_cursor"}.
## Emits `messages_updated`; `message_received` is reserved for live arrivals.
func history(channel_id: String, callback: Callable, limit: int = 0, cursor: String = "", forward: bool = true) -> void:
	if channel_id.is_empty() or not callback.is_valid():
		_fail(callback, "channel id and callback are required")
		return
	var parts := PackedStringArray()
	if limit > 0:
		parts.append("limit=%d" % limit)
	if not cursor.is_empty():
		parts.append("cursor=" + cursor.uri_encode())
	if not forward:
		parts.append("forward=false")
	_request(HTTPClient.METHOD_GET, _channel_path(channel_id), "&".join(parts), "",
		func(response): _finish_history(channel_id, response, callback))


## `POST {channel}/message`. A String `text` is wrapped as {"text": …}; a
## Dictionary or Array is sent verbatim as the `content` JSON the DTO demands.
## On success the stored message lands in the cache and both signals fire.
func send(channel_id: String, text: Variant, callback: Callable) -> void:
	if channel_id.is_empty() or not callback.is_valid():
		_fail(callback, "channel id and callback are required")
		return
	_request(HTTPClient.METHOD_POST, _channel_path(channel_id) + "/message", "",
		_content_body(text),
		func(response): _finish_message(channel_id, response, callback, true))


## `PUT {channel}/message/{message_id}` — replaces only the content, as the
## `UpdateRequest` DTO allows; on success the cache entry is swapped for the
## server's copy and `messages_updated` fires.
func edit(channel_id: String, message_id: String, text: Variant, callback: Callable) -> void:
	if channel_id.is_empty() or message_id.is_empty() or not callback.is_valid():
		_fail(callback, "channel id, message id and callback are required")
		return
	_request(HTTPClient.METHOD_PUT,
		"%s/message/%s" % [_channel_path(channel_id), message_id.uri_encode()], "",
		_content_body(text),
		func(response): _finish_message(channel_id, response, callback, false))


## `DELETE {channel}/message/{message_id}`; on success the cache drops the
## message, `messages_updated` fires and `data` is the deleted message.
func remove(channel_id: String, message_id: String, callback: Callable) -> void:
	if channel_id.is_empty() or message_id.is_empty() or not callback.is_valid():
		_fail(callback, "channel id, message id and callback are required")
		return
	_request(HTTPClient.METHOD_DELETE,
		"%s/message/%s" % [_channel_path(channel_id), message_id.uri_encode()], "", "",
		func(response): _finish_remove(channel_id, response, callback))


## Feeds a live message (an `apiChannelMessage` Dictionary or its JSON text)
## into the cache and the signals — the seam a socket transport calls later.
## Returns false when the payload is not a message shape.
func ingest_message(raw: Variant) -> bool:
	var source := raw
	if typeof(source) == TYPE_STRING or typeof(source) == TYPE_STRING_NAME:
		source = JSON.parse_string(source)
	if typeof(source) != TYPE_DICTIONARY:
		return false
	var message := _message_to_dict(source)
	var channel_id: String = message["channel_id"]
	if channel_id.is_empty():
		return false
	if _merge(channel_id, message):
		message_received.emit(message)
		messages_updated.emit(channel_id, messages_for(channel_id))
	return true


## A copy of the cached timeline for `channel_id`, oldest first.
func messages_for(channel_id: String) -> Array:
	var cached: Array = _cache.get(channel_id, [])
	return cached.duplicate()


## Drops the cached timeline for one channel, or every channel when empty.
func clear(channel_id: String = "") -> void:
	if channel_id.is_empty():
		_cache.clear()
	else:
		_cache.erase(channel_id)


func _channel_path(channel_id: String) -> String:
	return "/v2/channel/" + channel_id.uri_encode()


static func _content_body(text: Variant) -> String:
	var content: Variant = text
	if typeof(content) == TYPE_STRING or typeof(content) == TYPE_STRING_NAME:
		content = {"text": content}
	return JSON.stringify({"content": content})


static func _message_to_dict(raw: Variant) -> Dictionary:
	var out := {}
	if typeof(raw) != TYPE_DICTIONARY:
		return out
	out["channel_id"] = str(raw.get("channel_id", ""))
	out["message_id"] = str(raw.get("message_id", ""))
	out["code"] = int(raw.get("code", 0))
	out["sender_id"] = str(raw.get("sender_id", ""))
	out["username"] = str(raw.get("username", ""))
	var content_text := str(raw.get("content", ""))
	out["content"] = content_text
	out["content_json"] = JSON.parse_string(content_text) if not content_text.is_empty() else null
	out["create_time"] = str(raw.get("create_time", ""))
	out["update_time"] = str(raw.get("update_time", ""))
	out["persistent"] = bool(raw.get("persistent", false))
	out["room_name"] = str(raw.get("room_name", ""))
	out["group_id"] = str(raw.get("group_id", ""))
	out["user_id_one"] = str(raw.get("user_id_one", ""))
	out["user_id_two"] = str(raw.get("user_id_two", ""))
	return out


static func _message_list_to_dict(raw: Variant) -> Dictionary:
	var out := {"messages": []}
	if typeof(raw) != TYPE_DICTIONARY:
		return out
	for message in raw.get("messages", []):
		out["messages"].append(_message_to_dict(message))
	out["next_cursor"] = str(raw.get("next_cursor", ""))
	out["prev_cursor"] = str(raw.get("prev_cursor", ""))
	out["cacheable_cursor"] = str(raw.get("cacheable_cursor", ""))
	return out


## Replaces an existing message or appends a new one; true when appended.
func _merge(channel_id: String, message: Dictionary) -> bool:
	var cached: Array = _cache.get(channel_id, [])
	for i in range(cached.size()):
		if typeof(cached[i]) == TYPE_DICTIONARY \
				and str(cached[i].get("message_id", "")) == str(message.get("message_id", "")):
			cached[i] = message
			_cache[channel_id] = cached
			return false
	cached.append(message)
	if cached.size() > CACHE_LIMIT:
		cached = cached.slice(cached.size() - CACHE_LIMIT)
	_cache[channel_id] = cached
	return true


func _replace(channel_id: String, message: Dictionary) -> void:
	var cached: Array = _cache.get(channel_id, [])
	for i in range(cached.size()):
		if typeof(cached[i]) == TYPE_DICTIONARY \
				and str(cached[i].get("message_id", "")) == str(message.get("message_id", "")):
			cached[i] = message
			_cache[channel_id] = cached
			return


func _remove_cached(channel_id: String, message_id: String) -> void:
	var cached: Array = _cache.get(channel_id, [])
	for i in range(cached.size()):
		if typeof(cached[i]) == TYPE_DICTIONARY and str(cached[i].get("message_id", "")) == message_id:
			cached.remove_at(i)
			_cache[channel_id] = cached
			return


func _finish_history(channel_id: String, response: Dictionary, callback: Callable) -> void:
	if response.get("is_success", false):
		var data := _message_list_to_dict(response["json"])
		response["data"] = data
		for message in data["messages"]:
			_merge(channel_id, message)
		messages_updated.emit(channel_id, messages_for(channel_id))
	else:
		response["data"] = null
	callback.call(response)


func _finish_message(channel_id: String, response: Dictionary, callback: Callable, is_new: bool) -> void:
	if response.get("is_success", false):
		var message := _message_to_dict(response["json"])
		response["data"] = message
		var appended: bool = _merge(channel_id, message) if is_new else false
		if not appended and not is_new:
			_replace(channel_id, message)
		message_received.emit(message)
		messages_updated.emit(channel_id, messages_for(channel_id))
	else:
		response["data"] = null
	callback.call(response)


func _finish_remove(channel_id: String, response: Dictionary, callback: Callable) -> void:
	if response.get("is_success", false):
		var message := _message_to_dict(response["json"])
		response["data"] = message
		_remove_cached(channel_id, message["message_id"])
		messages_updated.emit(channel_id, messages_for(channel_id))
	else:
		response["data"] = null
	callback.call(response)


func _fail(callback: Callable, error: String) -> void:
	if callback.is_valid():
		callback.call({
			"is_success": false, "status_code": 0, "text": "", "json": null,
			"data": null, "error": error,
		})


## Runs one call through `CYGHttp`, the BaaS layer's one transport, and hands the
## callback the response shape the rest of this class already reads: `status_code`,
## `text`, `json`, `is_success` and `error`. Nothing above this line changes when the
## endpoint or the credential does.
func _request(method: int, path: String, query: String, body: String, on_response: Callable) -> void:
	# A query rides inside the path: `CYGHttp` forwards it verbatim, and history paging
	# (`?limit=&cursor=&forward=`) is the only place this class needs one.
	var endpoint := path if query.is_empty() else path + "?" + query
	CYGHttp.send_method(method, endpoint, body, true,
		func(reply: CYGHttp.Response) -> void:
			var is_success: bool = reply.is_success
			var error := ""
			if not reply.reached_server:
				error = reply.error
			elif not is_success:
				# The gateway's `error.message` rather than the whole envelope: this
				# string is what a game shows a player, and the envelope is machine
				# bookkeeping (`code`, `details`) on the way to it.
				var message := reply.error_message()
				if message.length() > _MAX_BODY_ERROR:
					message = message.substr(0, _MAX_BODY_ERROR) + "…"
				error = "http %d: %s" % [reply.status_code, message]
			on_response.call({
				"is_success": is_success,
				"status_code": reply.status_code,
				"text": reply.text,
				"json": reply.json,
				"data": null,
				"error": error,
			}))
