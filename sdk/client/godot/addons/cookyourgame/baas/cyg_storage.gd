class_name CYGStorage
extends RefCounted
## Cloud saves for the CookYourGame Godot SDK: the `/v2/storage*` document store.
##
## Objects are keyed by the triple `(collection, key, user_id)` and the server's
## own permission model is carried verbatim, so nothing here is cached, retried
## or invented: every method issues exactly one `HTTPRequest` and answers through
## the callback it was handed.
##
## Endpoints, field by field (see `crates/engine-api/src/controllers/storage.rs`,
## `crates/engine-api/src/dto/storage.rs` and `src/services/storage/`):
##
##  * `PUT /v2/storage` — batch write, applied in one transaction.
##
##        {"objects": [{"collection": str, "key": str, "value": str,
##                      "version": str, "permission_read": int,
##                      "permission_write": int}]}
##
##    `value` is the JSON *text* of the object and the gateway binds it as
##    `jsonb`, so it must parse and must start with `{`
##    (`INVALID_VALUE_MESSAGE` in `utils/storage.rs`). `version` is the
##    client-driven compare-and-set token: `""` upserts, `"*"` inserts only,
##    anything else is an if-match. Both permission fields are optional
##    (`serde(default)` + `Option`), and the server's own default is
##    `PERMIT_OWNER` = 1, which is what `set_value()` sends. Read accepts 0..2,
##    write accepts 0..1.
##
##    Reply is `StorageObjectAcks`: `{"acks": [{"collection", "key", "version",
##    "user_id", "create_time", "update_time"}]}`; the gateway drops an empty
##    repeated field, so a batch that wrote nothing answers `{}`.
##
##  * `POST /v2/storage` — batch read of named ids.
##
##        {"object_ids": [{"collection": str, "key": str,
##                         "user_id": str, "version": str}]}
##
##    An absent `user_id` (or `""`) means the caller's own account
##    (`read_owner()`); the nil UUID is refused. An object that does not exist,
##    or that the caller may not read, is simply ABSENT from the answer rather
##    than an error, and `objects` is omitted when empty — so "not found" is an
##    `ok` reply with `result == null`, the way `CYGStorage.ReadAsync` returns
##    null.
##
##    Reply is `StorageObjects`, then one `StorageObject`:
##
##        {"objects": [{"collection", "key", "user_id", "value", "version",
##                      "permission_read", "permission_write",
##                      "create_time", "update_time"}]}
##
##    `permission_read`/`permission_write` are skipped when zero, so they are
##    absent from the wire rather than `0`; `_normalize_object()` restores them.
##
##  * `PUT /v2/storage/delete` — batch delete, same `{"object_ids": [...]}` body
##    as the read (`version` may pin which state is removed). A row the caller
##    does not own, or one already gone, aborts the whole batch with
##    `Storage delete rejected.` Reply is `Empty`: `{}`.
##
##  * `GET /v2/storage/{collection}` — public listing of one collection, and
##    `GET /v2/storage/{collection}/{user_id}` the same listing scoped to a user.
##    Query is `ListQuery`: `?limit=<1..100>&cursor=<base64url>`, where `limit`
##    outside 1..100 is a 400 (`Invalid limit - limit must be between 1 and 100.`)
##    and `cursor` resumes after the last returned key. Reply is
##    `StorageObjectList`: `{"objects": [...], "cursor": str}` — both fields are
##    omitted when empty, so an exhausted page has no `cursor` at all.
##    `get_collection()` answers with the objects verbatim in `result` and the
##    page token in `cursor`.
##
## All callbacks receive one Dictionary, `ok` first and never absent:
##
##     {"ok": bool, "status": int, "error": String, "result": Variant,
##      "cursor": String, "object": Dictionary}
##
## `status` is the HTTP code, or 0 when the request never completed (DNS, refused
## connection, timeout); `error` is the `error.message` of the server's refusal
## body (`{"error": {"code", "message", "details"}}`, `error/v2.rs`) or the raw
## body when it is not that shape, and `error_string()` of the transport code when
## the request never reached the server; `object` carries the whole
## `StorageObject` a read matched (empty when nothing matched). `cursor` is only
## meaningful to `get_collection()`.
##
## Usage:
##
##     var save := {"level": 3, "hp": 90}
##     CYGStorage.set_value("saves", "slot1", save, func(r: Dictionary) -> void:
##         print("written: ", r["ok"]))
##
##     CYGStorage.get_value("saves", "slot1", func(r: Dictionary) -> void:
##         if r["ok"] and r["result"] != null:
##             _apply_save(r["result"])
##         else:
##             _apply_save(_default_save()))   # absent object or a failed read
##
##     CYGStorage.get_collection("saves", func(r: Dictionary) -> void:
##         for obj in r["result"]:
##             print(obj["key"], " ", obj["version"])
##
## Every call goes through `CYGHttp`, the BaaS layer's one transport: it resolves the
## endpoint, attaches `Authorization` from the session `CYGAuth` published, and retries
## a request that never got a reply, so a storage read and a login cannot disagree about
## any of that. A game that must point storage somewhere else sets `CYGHttp.base_url`.
##
## Independent port of this repo's Unity BaaS layer
## (`sdk/unity/Runtime/BaaS/CYGStorage.cs`) onto the documented `/v2/` endpoints
## of the Rust engine-api; no upstream source text is copied.

const PATH_WRITE := "/v2/storage"
const PATH_DELETE := "/v2/storage/delete"

const PERMISSION_PRIVATE := 0
const PERMISSION_OWNER := 1
const PERMISSION_PUBLIC := 2

## `MAX_LIST_LIMIT` in `utils/storage.rs`: the server 400s above it.
const MAX_LIST_LIMIT := 100



## Writes (or updates) one document. `value` must be a JSON object; it is
## `JSON.stringify`-ed here, since the server binds the text as `jsonb`.
## Answers `result` with the server's `acks` array.
static func set_value(
		collection: String,
		key: String,
		value: Variant,
		callback: Callable,
		permission_read: int = PERMISSION_OWNER,
		permission_write: int = PERMISSION_OWNER
) -> void:
	if collection.is_empty() or key.is_empty():
		_fail(callback, "collection and key must both be set.")
		return
	# `check_write()` in utils/storage.rs: read is 0..2, write is 0..1.
	if permission_read < PERMISSION_PRIVATE or permission_read > PERMISSION_PUBLIC:
		_fail(callback, "permission_read must be 0, 1 or 2, got %d." % permission_read)
		return
	if permission_write < PERMISSION_PRIVATE or permission_write > PERMISSION_OWNER:
		_fail(callback, "permission_write must be 0 or 1, got %d." % permission_write)
		return
	if typeof(value) != TYPE_DICTIONARY:
		_fail(callback, "a storage value must be a JSON object (Dictionary), got %s."
				% type_string(typeof(value)))
		return

	var encoded := JSON.stringify(value)
	# The server rejects an empty value before it looks at the JSON, and a
	# Dictionary with no pairs encodes as "{}" which it accepts.
	if encoded.is_empty() or not encoded.strip_edges().begins_with("{"):
		_fail(callback, "value did not encode to a JSON object.")
		return

	var body := JSON.stringify({
		"objects": [
			{
				"collection": collection,
				"key": key,
				"value": encoded,
				"permission_read": permission_read,
				"permission_write": permission_write,
			}
		]
	})
	_send(HTTPClient.METHOD_PUT, PATH_WRITE, body, callback, _acks_reply)


## Reads one document of the caller's own account (or of `user_id` when given).
## `result` is the decoded value, `null` when the object is absent or not readable;
## `object` is the whole `StorageObject` the server matched.
static func get_value(collection: String, key: String, callback: Callable, user_id: String = "") -> void:
	if collection.is_empty() or key.is_empty():
		_fail(callback, "collection and key must both be set.")
		return
	var id := {"collection": collection, "key": key}
	if not user_id.is_empty():
		id["user_id"] = user_id
	var body := JSON.stringify({"object_ids": [id]})
	_send(HTTPClient.METHOD_POST, PATH_WRITE, body, callback, _objects_reply)


## Pages one collection. `limit` is clamped to the server's own 1..100 window and
## `cursor` is the token a previous page returned; an empty `cursor` in the answer
## means the listing is exhausted.
static func get_collection(collection: String, callback: Callable, limit: int = MAX_LIST_LIMIT, cursor: String = "") -> void:
	if collection.is_empty():
		_fail(callback, "collection must be set.")
		return
	if limit < 1 or limit > MAX_LIST_LIMIT:
		_fail(callback, "limit must be between 1 and %d, got %d." % [MAX_LIST_LIMIT, limit])
		return
	var query := "?limit=%d" % limit
	if not cursor.is_empty():
		query += "&cursor=%s" % cursor
	_send(HTTPClient.METHOD_GET, PATH_WRITE + "/" + collection.uri_encode() + query, "", callback, _listing_reply)


## Deletes one document, the counterpart of `CYGStorage.DeleteAsync`. The server
## deletes the caller's own rows only, and refuses a row that is already gone by
## failing the batch with `Storage delete rejected.`
static func delete_key(collection: String, key: String, callback: Callable) -> void:
	delete_keys([{"collection": collection, "key": key}], callback)


## Batch delete. `object_ids` is a Dictionary per row, exactly the `ObjectId` DTO:
## `{"collection", "key", "user_id" (ignored, delete is always own), "version"}`;
## a `version` names the exact state to remove. There is no whole-collection
## delete on `/v2/storage` and none in the Unity class, so there is none here.
static func delete_keys(object_ids: Array, callback: Callable) -> void:
	if object_ids.is_empty():
		_fail(callback, "delete_keys needs at least one object id.")
		return
	for entry in object_ids:
		if typeof(entry) != TYPE_DICTIONARY \
				or str(entry.get("collection", "")).is_empty() \
				or str(entry.get("key", "")).is_empty():
			_fail(callback, "every object id needs a collection and a key.")
			return
	# The service sorts and applies the batch itself; send the caller's order.
	var body := JSON.stringify({"object_ids": object_ids})
	_send(HTTPClient.METHOD_PUT, PATH_DELETE, body, callback, _empty_reply)


## Decodes a `StorageObject`'s `value` text. The store keeps JSON objects, so a
## malformed row yields `null` rather than an exception.
static func parse_value(object: Dictionary) -> Variant:
	return JSON.parse_string(str(object.get("value", "")))


## Restores the two permission fields the gateway omits when they are zero.
static func _normalize_object(object: Dictionary) -> Dictionary:
	var normalized := object.duplicate()
	normalized["permission_read"] = int(object.get("permission_read", PERMISSION_PRIVATE))
	normalized["permission_write"] = int(object.get("permission_write", PERMISSION_PRIVATE))
	return normalized


## `{"objects": [...]}`: hands back the first match's decoded value.
static func _objects_reply(reply: Dictionary, response: Dictionary) -> void:
	var objects: Array = response.get("objects", [])
	if objects.is_empty() or typeof(objects[0]) != TYPE_DICTIONARY:
		reply["result"] = null
		reply["object"] = {}
		return
	var first := _normalize_object(objects[0] as Dictionary)
	reply["object"] = first
	reply["result"] = parse_value(first)


## `{"acks": [...]}`: hands the acknowledgement rows back untouched.
static func _acks_reply(reply: Dictionary, response: Dictionary) -> void:
	reply["result"] = response.get("acks", [])


## A listing hands back every `StorageObject` with its zero permissions restored.
static func _listing_reply(reply: Dictionary, response: Dictionary) -> void:
	var listed: Array = []
	for entry in response.get("objects", []):
		if typeof(entry) == TYPE_DICTIONARY:
			listed.append(_normalize_object(entry as Dictionary))
	reply["result"] = listed


## `PUT /v2/storage/delete` answers `Empty`, so there is nothing to refine.
static func _empty_reply(_reply: Dictionary, _response: Dictionary) -> void:
	pass


static func _fail(callback: Callable, message: String) -> void:
	_deliver(callback, _reply_dict(false, 0, message))


static func _reply_dict(ok: bool, status: int, error: String) -> Dictionary:
	return {"ok": ok, "status": status, "error": error, "result": null,
			"cursor": "", "object": {}}


static func _deliver(callback: Callable, reply: Dictionary) -> void:
	if callback.is_valid():
		callback.call(reply)


## Runs one BaaS request and turns its answer into the reply Dictionary. `refine`
## fills `result` from the decoded envelope; it runs only on a 2xx.
static func _send(method: int, path: String, body: String, callback: Callable, refine: Callable) -> void:
	_request(method, path, body,
		func(status: int, text: String, transport_error: String) -> void:
			var ok: bool = status >= 200 and status <= 299 and transport_error.is_empty()
			var reply := _reply_dict(ok, status,
					"" if ok else _first_non_empty(transport_error, error_message_of(text)))
			if ok:
				var parsed := JSON.parse_string(text)
				if typeof(parsed) == TYPE_DICTIONARY:
					var response := parsed as Dictionary
					reply["cursor"] = str(response.get("cursor", ""))
					if refine.is_valid():
						refine.call(reply, response)
				else:
					# A 2xx that is not a JSON object cannot be trusted: `Empty`
					# replies do, anything else is reported as a bad answer.
					var stripped := text.strip_edges()
					if stripped != "" and stripped != "{}":
						reply["ok"] = false
						reply["error"] = "storage reply was not a JSON object: " + text
			_deliver(callback, reply))


static func _first_non_empty(first: String, second: String) -> String:
	return first if not first.is_empty() else second


## A `/v2/` refusal is `{"error": {"code", "message", "details"}}`
## (`error/v2.rs`); anything else is surfaced as the raw body.
static func error_message_of(text: String) -> String:
	if text.is_empty():
		return ""
	var parsed := JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		return text
	var details: Variant = (parsed as Dictionary).get("error", null)
	if typeof(details) != TYPE_DICTIONARY:
		return text
	var message := str((details as Dictionary).get("message", ""))
	return message if not message.is_empty() else text


## Runs one call through `CYGHttp` and keeps this class's own reply shape: `status` is
## the HTTP code, or 0 when no reply arrived, in which case `transport_error` says why.
## The `HTTPClient.METHOD_*` code the callers already hold goes straight to the transport
## rather than through a verb string that would only be translated back.
static func _request(method: int, path: String, body: String, on_done: Callable) -> void:
	CYGHttp.send_method(method, path, body, true,
		func(response: CYGHttp.Response) -> void:
			on_done.call(response.status_code, response.text,
				"" if response.reached_server else response.error))
