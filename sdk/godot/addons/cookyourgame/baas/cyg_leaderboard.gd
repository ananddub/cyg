class_name CYGLeaderboard
extends RefCounted
## Competitive leaderboards over the room server's `/v2/leaderboard` routes.
##
## Mirrors `CYGLeaderboard` in the Unity SDK (`Runtime/BaaS/CYGLeaderboard.cs`)
## against the routes the Rust engine actually serves
## (`crates/engine-api/src/controllers/leaderboard.rs`) and the record shapes in
## `crates/engine-api/src/dto/leaderboard.rs`.
##
## The gateway prints int64s (score, subscore, rank, rank_count) as decimal
## strings and omits every zero-valued field, so an absent key reads as `0`,
## never as `null`. Timestamps are RFC 3339 at whole seconds.
##
## Endpoints, exactly as the server declares them:
##
##  * `POST /v2/leaderboard/{leaderboard_id}` — upsert the caller's record.
##    Request body (`RecordRequest`):
##
##        {"score": <int64>, "subscore": <int64>,
##         "metadata": "<json string>", "operator": <int64>}
##
##    `score`/`subscore` may be a bare or a quoted int64. `metadata` is a JSON
##    *string*, not an object. `operator` selects the write mode; 0 is the
##    default "set" operator. Replies with one `RecordResponse`:
##
##        {"leaderboard_id", "owner_id", "username"?, "score"?, "subscore"?,
##         "num_score"?, "metadata", "create_time", "update_time",
##         "expiry_time"?, "rank"?, "max_num_score"?}
##
##  * `GET /v2/leaderboard/{leaderboard_id}` — a page of records.
##    Query (`ListQuery`): `limit` (1..=MAX, absent means 1 unless owners are
##    named), `cursor`, `owner_ids` (comma-separated or repeated), `expiry`
##    (unix-seconds override of the board's reset window). Absent `limit` and
##    `expiry` are distinct from zero, so this class omits them when unset.
##    Replies `RecordListResponse`:
##
##        {"records"?: [RecordResponse], "owner_records"?: [RecordResponse],
##         "next_cursor"?, "prev_cursor"?, "rank_count"?}
##
##  * `GET /v2/leaderboard/{leaderboard_id}/owner/{owner_id}` — the haystack:
##    the owner's record with its neighbours, i.e. the around-rank window.
##    Query (`HaystackQuery`): `limit`, `cursor`, `expiry` (same semantics;
##    passing a cursor turns it into an ordinary page). Replies
##    `RecordListResponse`. `owner_id` must be a UUID.
##
##  * `DELETE /v2/leaderboard/{leaderboard_id}` — deletes the caller's own
##    record inside the board's current reset window. Replies `{}` (Empty).
##
## Weekly boards: the DTO expresses no period field — a board's reset schedule
## lives on the leaderboard itself, and each read/write is automatically bound
## to the window the schedule names (`record_window` server-side). The only
## client-side window knob is the `expiry` query override on the reads, which
## `list()` and `around_owner()` expose verbatim; there is nothing to invent.
##
## Callback contract, every public method:
##
##     callback(response: Dictionary)  # {is_success: bool, status_code: int,
##                                      #  text: String, json: Variant,
##                                      #  data: Variant, error: String}
##
## On success `data` is the parsed shape (`Dictionary`) named per endpoint; on
## any failure — transport or non-2xx — `is_success` is false, `error` names it
## and `data` is null. The callback always runs exactly once. Ordinary non-2xx
## answers are not pushed as engine errors; callers judge `is_success`.
##
## Transport: `CYGHttp`, the BaaS layer's one transport. It resolves the endpoint,
## attaches the session `CYGAuth` published and retries a request that never got a
## reply, so a submit and a list cannot disagree about any of that.
##
## Usage:
##
##     CYGHttp.base_url = "http://127.0.0.1:8090"   # optional; see `CYGAuth`
##
##     func on_submit(response: Dictionary) -> void:
##         if response["is_success"]:
##             print("rank ", response["data"]["rank"])
##     func _ready() -> void:
##         CYGLeaderboard.submit("weekly-deathmatch", 4200, {"mode": "hard"},
##             on_submit)
##         CYGLeaderboard.list("weekly-deathmatch", on_top, 25)
##         CYGLeaderboard.rank_of("weekly-deathmatch", my_uuid, on_my_rank)
##
## Independent GDScript port of this repo's Unity/Unreal BaaS layer onto the
## documented `/v2/` endpoints of `crates/engine-api`; no upstream source text
## is copied.

const _MAX_BODY_ERROR := 240

# `data` parsers selected inside `_request`; mirrors how the Rust controller
# picks the DTO per route.
enum _Mode {
	RAW,       # `data` = whatever JSON the route returned.
	RECORD,    # one `RecordResponse`, normalized.
	RECORDS,   # a `RecordListResponse`, normalized.
	RANK,      # haystack narrowed to {"rank": int, "record": Dictionary}.
	EMPTY,     # success answers only; `data` = {}.
}

static func _record_to_dict(raw: Variant) -> Dictionary:
	var out := {}
	if typeof(raw) != TYPE_DICTIONARY:
		return out
	out["leaderboard_id"] = str(raw.get("leaderboard_id", ""))
	out["owner_id"] = str(raw.get("owner_id", ""))
	out["username"] = str(raw.get("username", ""))
	out["score"] = int(str(raw.get("score", "0")))
	out["subscore"] = int(str(raw.get("subscore", "0")))
	out["num_score"] = int(str(raw.get("num_score", "0")))
	out["metadata"] = str(raw.get("metadata", ""))
	out["create_time"] = str(raw.get("create_time", ""))
	out["update_time"] = str(raw.get("update_time", ""))
	out["expiry_time"] = str(raw.get("expiry_time", ""))
	out["rank"] = int(str(raw.get("rank", "0")))
	out["max_num_score"] = int(str(raw.get("max_num_score", "0")))
	return out


static func _records_to_dict(raw: Variant) -> Dictionary:
	var out := {"records": [], "owner_records": []}
	if typeof(raw) != TYPE_DICTIONARY:
		return out
	out["next_cursor"] = str(raw.get("next_cursor", ""))
	out["prev_cursor"] = str(raw.get("prev_cursor", ""))
	out["rank_count"] = int(str(raw.get("rank_count", "0")))
	for record in raw.get("records", []):
		out["records"].append(_record_to_dict(record))
	for record in raw.get("owner_records", []):
		out["owner_records"].append(_record_to_dict(record))
	return out


static func _rank_data(raw: Variant) -> Dictionary:
	var listed := _records_to_dict(raw)
	var out := {"rank": 0, "record": {}}
	if not listed["owner_records"].is_empty():
		out["record"] = listed["owner_records"][0]
		out["rank"] = int(out["record"]["rank"])
	return out


static func _parse(mode: int, raw: Variant) -> Variant:
	match mode:
		_Mode.RECORD:
			return _record_to_dict(raw)
		_Mode.RECORDS:
			return _records_to_dict(raw)
		_Mode.RANK:
			return _rank_data(raw)
		_Mode.EMPTY:
			return {}
		_:
			return raw


static func _encode_score(value: Variant) -> Variant:
	if typeof(value) == TYPE_STRING or typeof(value) == TYPE_STRING_NAME:
		return value
	return int(value)


static func _encode_metadata(extra: Variant) -> String:
	# `metadata` is a JSON *string* on the wire (`Option<String>` in Rust);
	# Dictionary and Array are serialized, text is passed through untouched.
	if typeof(extra) == TYPE_STRING or typeof(extra) == TYPE_STRING_NAME:
		return extra
	if extra == null:
		return "{}"
	return JSON.stringify(extra)


static func _list_query(limit: int, cursor: String, owner_ids: PackedStringArray, expiry: String) -> String:
	var parts := PackedStringArray()
	if limit > 0:
		parts.append("limit=%d" % limit)
	if not cursor.is_empty():
		parts.append("cursor=" + cursor.uri_encode())
	if not owner_ids.is_empty():
		parts.append("owner_ids=" + ",".join(owner_ids).uri_encode())
	if not expiry.is_empty():
		parts.append("expiry=" + expiry.uri_encode())
	return "&".join(parts)


## `POST /v2/leaderboard/{leaderboard_id}`. Upserts the caller's record with
## `value` as `score`, `extra` as the `metadata` JSON string (Dictionary/Array
## is stringified; a String is sent as-is), and optional `subscore`/`operator`
## (0 = the board's default "set"; the server rejects operators its config
## forbids). `data` on success: the written `RecordResponse`.
static func submit(leaderboard_id: String, value: Variant, extra: Variant, callback: Callable, subscore: int = 0, write_operator: int = 0) -> void:
	if leaderboard_id.is_empty() or not callback.is_valid():
		_fail(callback, "leaderboard id and callback are required")
		return
	var body := JSON.stringify({
		"score": _encode_score(value),
		"subscore": subscore,
		"metadata": _encode_metadata(extra),
		"operator": write_operator,
	})
	_request(HTTPClient.METHOD_POST, "/v2/leaderboard/" + leaderboard_id.uri_encode(), "", body, callback, _Mode.RECORD)


## `GET /v2/leaderboard/{leaderboard_id}` with the real `ListQuery` knobs:
## `limit` (omit when 0 — absent means "1" server-side unless owners are named),
## `cursor`, `owner_ids` (batch of named owners, sent comma-separated) and
## `expiry` (unix-seconds string overriding the reset window). `data` on
## success: {"records", "owner_records", "next_cursor", "prev_cursor",
## "rank_count"}.
static func list(leaderboard_id: String, callback: Callable, limit: int = 0, cursor: String = "", owner_ids: PackedStringArray = PackedStringArray(), expiry: String = "") -> void:
	if leaderboard_id.is_empty() or not callback.is_valid():
		_fail(callback, "leaderboard id and callback are required")
		return
	var query := _list_query(limit, cursor, owner_ids, expiry)
	_request(HTTPClient.METHOD_GET, "/v2/leaderboard/" + leaderboard_id.uri_encode(), query, "", callback, _Mode.RECORDS)


## `GET /v2/leaderboard/{leaderboard_id}/owner/{owner_id}` — the around-owner
## (haystack) window: the owner's record plus `limit` neighbours on each side.
## Pass a `cursor` to page on as an ordinary listing (the route answers
## `RecordListResponse` either way). `data` on success: a `RecordListResponse`
## shape, same as `list()`.
static func around_owner(leaderboard_id: String, owner_id: String, callback: Callable, limit: int = 0, cursor: String = "", expiry: String = "") -> void:
	if leaderboard_id.is_empty() or owner_id.is_empty() or not callback.is_valid():
		_fail(callback, "leaderboard id, owner id and callback are required")
		return
	var query := _list_query(limit, cursor, PackedStringArray(), expiry)
	_request(HTTPClient.METHOD_GET,
		"/v2/leaderboard/" + leaderboard_id.uri_encode() + "/owner/" + owner_id.uri_encode(),
		query, "", callback, _Mode.RECORDS)


## Convenience over the haystack route: `data` on success is
## {"rank": int, "record": Dictionary}; `rank` is 0 when the owner holds no
## record in the board's current window.
static func rank_of(leaderboard_id: String, owner_id: String, callback: Callable) -> void:
	if leaderboard_id.is_empty() or owner_id.is_empty() or not callback.is_valid():
		_fail(callback, "leaderboard id, owner id and callback are required")
		return
	_request(HTTPClient.METHOD_GET,
		"/v2/leaderboard/" + leaderboard_id.uri_encode() + "/owner/" + owner_id.uri_encode(),
		"limit=1", "", callback, _Mode.RANK)


## `DELETE /v2/leaderboard/{leaderboard_id}` — removes the caller's own record
## for the window the board's reset schedule names (a tournament board answers
## not-found here, exactly as the server documents). `data` on success: `{}`.
static func delete(leaderboard_id: String, callback: Callable) -> void:
	if leaderboard_id.is_empty() or not callback.is_valid():
		_fail(callback, "leaderboard id and callback are required")
		return
	_request(HTTPClient.METHOD_DELETE, "/v2/leaderboard/" + leaderboard_id.uri_encode(), "", "", callback, _Mode.EMPTY)


static func _fail(callback: Callable, error: String) -> void:
	if callback.is_valid():
		callback.call({
			"is_success": false, "status_code": 0, "text": "", "json": null,
			"data": null, "error": error,
		})


static func _request(method: int, path: String, query: String, body: String, callback: Callable, mode: int = _Mode.RAW) -> void:
	# The query travels inside the path: `CYGHttp` forwards a path verbatim, and a
	# leaderboard list is exactly the kind of call that has one.
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
			callback.call({
				"is_success": is_success,
				"status_code": reply.status_code,
				"text": reply.text,
				"json": reply.json,
				"data": _parse(mode, reply.json) if is_success else null,
				"error": error,
			}))
