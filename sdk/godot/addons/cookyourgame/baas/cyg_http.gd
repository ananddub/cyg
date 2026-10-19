class_name CYGHttp
extends RefCounted
## Internal HTTP transport for the CookYourGame BaaS layer (auth, storage,
## inventory, leaderboard) — the control plane's `/v2` REST surface.
##
## Every call is one short-lived `HTTPRequest` node: created for the request,
## answered by `request_completed`, freed on the reply. Nothing blocks and
## nothing spins: a caller either hands over a `Callable` and forgets the call,
## or `await`s the same function and gets a `CYGHttp.Response` back. Both styles
## run the exact same code path, so a game can migrate between them per call.
##
## The url is `resolve_base_url() + path`: the base loses its trailing `/` and
## the path gains exactly one leading `/`, which is the join the Unity SDK's
## `CYGHttp.SendAsync` performs, so a `/v2/...` path written by hand lands on
## the same route in every SDK of this repo. A query string travels inside
## `path`, because the endpoints that take one (`?create=&username=`) are
## assembled by `CYGAuth` and no other layer needs to build them.
##
## Headers, in the order they are attached, and why each is there:
##   `Accept: application/json`              — always.
##   `server-key: <server_key>`              — whenever `server_key` is not empty.
##   `Content-Type: application/json`        — only when a body is sent.
##   `Authorization: …`                      — never more than one of these:
##     * `Bearer <session_token>` when `require_auth` is true and `CYGAuth` has
##       published a token. This is what `session_claims()` in
##       `crates/engine-api/src/app/middleware.rs` reads; the prefix is
##       case-sensitive, so `bearer …` is no credential at all.
##     * `Basic base64("<server_key>:")` when `require_auth` is false. The
##       server-key routes (`Authenticate*`, `SessionRefresh`) prove the caller
##       with the server key, which is the *user* half of a Basic credential
##       whose password half is the empty string — the trailing colon is part
##       of the credential, and the parser rejects a payload without one.
##
## Exactly one `Authorization` header, never two: `server_key()` and
## `session_claims()` both start by counting `Authorization` headers and refuse
## anything that is not exactly one, so a request carrying the server key *and*
## a bearer token is an unauthenticated failure rather than a doubly-proved
## caller. That is why `require_auth` picks a credential instead of adding one,
## and why the `server-key:` header exists alongside rather than inside it: the
## engine ignores that header today (it only ever reads `Authorization`), so it
## is sent for a proxy or a game relay in front of the plane that wants the key
## spelled out, and it costs one line either way.
##
## The reply body is the engine's own shape. Success is the payload of the
## route (`SessionResponse` for auth: `{"token":…,"refresh_token":…}`, with
## `created` omitted entirely when it is false). Failure is the envelope
## `{"error":{"code":<grpc code>,"message":"…","details":[…]}}` at the HTTP
## status `ErrorKind` chooses — 400, 401, 404, 409, 500.
##
## A non-2xx reply is *not* an error here: `is_success` goes false and the
## status, text and parsed `json` stay intact for the caller to judge, because
## a 409 from a username clash is a game-design answer, not a bug, and
## `push_error` would spam the console for every refused login. The one
## exception worth naming is a transport failure: `result` carries the
## `HTTPRequest.Result`, `status_code` stays 0, and `error` holds the reason.
##
## Retry is deliberately narrow. A request whose reply never arrived — a
## refused connection, a name that did not resolve, a timeout — is sent once
## more after `retry_delay_seconds`, because a first packet on a cold network
## stack is the failure this actually fixes. Any reply with a status code,
## including every 4xx and 5xx, is returned exactly as it arrived: a server
## that said "no" was reached, and repeating the question is the game's call.
##
## Usage:
##
##     # callback style — nothing blocks the game thread
##     CYGHttp.get_json("/v2/storage/find?key=profile", _on_profile)
##
##     # await style, from any node script
##     var reply: CYGHttp.Response = await CYGHttp.post_json(
##         "/v2/account/authenticate/device?create=true", '{"id":"abc"}', null, false)
##     if not reply.is_success:
##         print("refused: ", reply.error_message())
##
## Independent GDScript port of this repo's own SDKs (`sdk/unity/Runtime/BaaS/
## CYGHttp.cs` and `sdk/unreal/.../BaaS/CYGBaasHttp.cpp`); no upstream source
## text is copied. Nothing here derives from Netfox — the BaaS layer has no
## Netfox lineage, so this file carries no LICENSE-NETFOX.md attribution.

## Base url used when neither `base_url` nor a `CYGServerSettings` is set: the
## `web_port` this repo's `config.toml` ships with, which is where the engine
## serves its HTTP surface. A title that points somewhere else sets `base_url`.
const DEFAULT_BASE_URL := "http://127.0.0.1:8090"
## Seconds a request may take before `HTTPRequest` reports `RESULT_TIMEOUT`.
## Mirrors `CYGHttp.TimeoutSeconds` in the Unity SDK; it is a float here because
## Godot's `timeout` property is one.
const DEFAULT_TIMEOUT_SECONDS := 15.0
## Pause before the single retry of a connection-level failure.
const DEFAULT_RETRY_DELAY_SECONDS := 0.5
## The engine's out-of-the-box `control.session.server_key`
## (`crates/engine-config/src/session.rs`), so a default server authenticates a
## default client. A shipped title must overwrite it.
const DEFAULT_SERVER_KEY := "defaultkey"
## Header the engine's gateway itself does not read; kept for a front proxy.
const SERVER_KEY_HEADER := "server-key"

## Overrides the endpoint while non-empty. Set it before the first call.
static var base_url: String = ""
## Resource to read `server_http_url` from when `base_url` is empty; falls back
## to the `CYGNetwork` autoload's own settings, then to `DEFAULT_BASE_URL`.
static var settings: CYGServerSettings = null
## The `Basic` user half sent on server-key routes, and the value of the
## `server-key:` header. Empty means "send no credential at all".
static var server_key: String = DEFAULT_SERVER_KEY
## Request deadline in seconds; clamped to at least 1 by `HTTPRequest`.
static var timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS
## Wait before the one retry of a connection-level failure.
static var retry_delay_seconds: float = DEFAULT_RETRY_DELAY_SECONDS
## Bearer token published by `CYGAuth` for the signed-in player; "" after a
## logout. Read here so this layer never has to depend on the auth class.
static var session_token: String = ""
## Node in the tree the transient `HTTPRequest` is added to. Left null, the
## `CYGNetwork` autoload is used, then the scene root.
static var host: Node = null

static var _active: Array = []


## ------------------------------------------------------------------ the reply

class Response extends RefCounted:
	## `HTTPRequest.Result` for the attempt that produced this reply; 0 is
	## `RESULT_SUCCESS`, and only a transport failure is anything else.
	var result: int = HTTPRequest.Result.RESULT_SUCCESS
	## HTTP status, or 0 when no reply arrived at all.
	var status_code: int = 0
	## Body decoded as UTF-8, whether or not it parses as JSON.
	var text: String = ""
	## Raw body bytes, for a route that answers with something other than JSON.
	var bytes: PackedByteArray = PackedByteArray()
	## Transport-level reason (`Godot`'s `error_string()`) or a local refusal
	## such as "no endpoint configured"; "" when a reply actually arrived.
	var error: String = ""
	## True when this reply is the second attempt of a connection-level failure.
	var retried: bool = false
	## `text` parsed as JSON: a `Dictionary` for an object reply, `null` when
	## the body was not JSON, and not necessarily an object either way.
	var json: Variant = null

	## Reached the server and it answered 2xx. A refusal is false without being
	## an error, and nothing in this class reports one for a refusal.
	var is_success: bool:
		get:
			if not error.is_empty():
				return false
			return result == HTTPRequest.Result.RESULT_SUCCESS \
				and status_code >= 200 and status_code <= 299

	## The reply as a `Dictionary`, or an empty one when it was not a JSON
	## object. Lets a caller chain `.get()` without a `typeof` guard each time.
	var dictionary: Dictionary:
		get:
			return json if typeof(json) == TYPE_DICTIONARY else {}

	## `error.message` from the engine's failure envelope, falling back to the
	## raw body and then to the transport error, so there is always something
	## worth logging. Never reads as an error when the reply is a success.
	func error_message() -> String:
		var body: Variant = dictionary.get("error")
		if typeof(body) == TYPE_DICTIONARY:
			var message := str(body.get("message", ""))
			if not message.is_empty():
				return message
		if not text.is_empty():
			return text
		if not error.is_empty():
			return error
		return "HTTP %d" % status_code

	## `error.code` from the failure envelope: the gRPC status number the engine
	## chose (3 invalid argument, 16 unauthenticated, 5 not found, 6 already
	## exists), or 0 when the reply carried no envelope.
	func error_code() -> int:
		var body: Variant = dictionary.get("error")
		if typeof(body) == TYPE_DICTIONARY:
			return int(body.get("code", 0))
		return 0

	## True when a status code arrived at all: 0 means the server was never
	## reached, which no 2xx check can distinguish from a live refusal.
	var reached_server: bool:
		get:
			return status_code > 0

	func _to_string() -> String:
		if status_code == 0:
			return "[CYGHttp.Response unreachable %s]" % (error if not error.is_empty() else str(result))
		return "[CYGHttp.Response %d%s]" % [status_code, "" if is_success else " " + error_message().left(80)]


## ------------------------------------------------------------------- the calls

## GET with a JSON reply. `on_done` is optional: pass a `Callable(Response)` and
## the call is fire-and-forget, or `await` the returned `Response`.
static func get_json(path: String, on_done: Callable = Callable(), require_auth: bool = true) -> Response:
	return await _dispatch(HTTPClient.METHOD_GET, path, "", require_auth, on_done)


## POSTs `body`, a JSON string; "" sends no body at all (the engine reads that
## as an absent message, which some routes accept and some refuse).
static func post_json(path: String, body: String, on_done: Callable = Callable(), require_auth: bool = true) -> Response:
	return await _dispatch(HTTPClient.METHOD_POST, path, body, require_auth, on_done)


static func put_json(path: String, body: String, on_done: Callable = Callable(), require_auth: bool = true) -> Response:
	return await _dispatch(HTTPClient.METHOD_PUT, path, body, require_auth, on_done)


## DELETE. Named `del_json` — `delete` is a keyword, and the Unity SDK's
## `DeleteAsync` has no more room for a comment than this does.
static func del_json(path: String, on_done: Callable = Callable(), require_auth: bool = true) -> Response:
	return await _dispatch(HTTPClient.METHOD_DELETE, path, "", require_auth, on_done)


## The verb-taking form every helper above funnels through, and the one a route
## this SDK has not wrapped yet needs.
static func send(verb: String, path: String, body: String = "", require_auth: bool = true,
		on_done: Callable = Callable()) -> Response:
	return await _dispatch(_method_code(verb), path, body, require_auth, on_done)


## The `HTTPClient.METHOD_*` form of `send`, for the BaaS classes that already hold an
## engine method code: naming a verb string here would only be a translation on the way
## back to the number the request needs.
static func send_method(method: int, path: String, body: String = "", require_auth: bool = true,
		on_done: Callable = Callable()) -> Response:
	return await _dispatch(method, path, body, require_auth, on_done)


## One attempt, then one more when the first never got a reply. Every caller
## above is a coroutine, so calling one without `await` starts it and returns
## immediately; the resume happens on `request_completed`.
static func _dispatch(method: int, path: String, body: String, require_auth: bool,
		on_done: Callable) -> Response:
	var response: Response = await _perform(method, path, body, require_auth)
	if _is_connection_failure(response):
		await _sleep(retry_delay_seconds)
		response = await _perform(method, path, body, require_auth)
		response.retried = true
	if on_done.is_valid():
		on_done.call(response)
	return response


## True only for a failure to obtain a reply at all. Godot reports
## `RESULT_SUCCESS` for every HTTP status the server actually sent — a 404 and a
## 500 arrive as a successful *transfer* with the status in `response_code` — so
## this test cannot fire on a 4xx, which is the whole point of gating the retry
## on it rather than on a status range.
static func _is_connection_failure(response: Response) -> bool:
	return response.status_code == 0 \
		and response.result != HTTPRequest.Result.RESULT_SUCCESS


## --------------------------------------------------------------- one attempt

static func _perform(method: int, path: String, body: String, require_auth: bool) -> Response:
	var response := Response.new()
	var url := resolve_url(path)
	if url.is_empty():
		push_warning("CYGHttp: no endpoint configured, so nothing was sent for '%s'." % path)
		response.error = "no endpoint configured"
		return response

	var node := _host_node()
	if node == null:
		response.error = "no node in the tree can host the request"
		push_warning("CYGHttp: %s." % response.error)
		return response

	var http := HTTPRequest.new()
	http.timeout = maxf(timeout_seconds, 1.0)
	http.use_threads = true
	node.add_child(http)
	_active.append(http)

	var started: int = http.request(url, build_headers(require_auth, body), method, body)
	if started != OK:
		_retire(http)
		# A refused `request()` is a bad argument or a dead node, not a network
		# hiccup: retrying it would fail the same way, so `result` stays
		# RESULT_SUCCESS and `_is_connection_failure` finds nothing to retry.
		response.error = error_string(started)
		push_warning("CYGHttp: the request for '%s' could not start (%d: %s)."
			% [path, started, response.error])
		return response

	var completed: Array = await http.request_completed
	_retire(http)

	response.result = int(completed[0])
	response.status_code = int(completed[1])
	response.bytes = completed[3] as PackedByteArray
	response.text = response.bytes.get_string_from_utf8()
	# An empty body is a valid 2xx answer, so it is never handed to the parser:
	# `JSON.parse_string("")` reports a parse error of its own.
	if not response.text.strip_edges().is_empty():
		response.json = JSON.parse_string(response.text)
	if response.result != HTTPRequest.Result.RESULT_SUCCESS:
		response.error = _result_name(response.result)
	return response


## ------------------------------------------------------------------- headers

## Builds the header list for one call. Exposed for a game that must log or
## assert what went on the wire; the ordering is the documented one.
static func build_headers(require_auth: bool, body: String) -> PackedStringArray:
	var headers := PackedStringArray(["Accept: application/json"])
	if not server_key.is_empty():
		headers.append("%s: %s" % [SERVER_KEY_HEADER, server_key])
	if not body.is_empty():
		headers.append("Content-Type: application/json")
	var bearer := session_token if require_auth else ""
	if not bearer.is_empty():
		headers.append("Authorization: Bearer " + bearer)
	elif not require_auth and not server_key.is_empty():
		headers.append("Authorization: Basic " + server_key_credential())
	return headers


## `base64("<server_key>:")` — the colon belongs to the credential, the password
## half is deliberately empty, and a payload without the colon is rejected.
static func server_key_credential() -> String:
	return Marshalls.raw_to_base64((server_key + ":").to_utf8_buffer())


## Publishes the signed-in player's bearer token. `CYGAuth` owns the value;
## going through this setter is what keeps the transport from depending on it.
static func publish_token(token: String) -> void:
	session_token = token


## ------------------------------------------------------------------- the base

## The endpoint this layer would call: `base_url` if set, else `settings` or the
## `CYGNetwork` autoload's `server_http_url`, else `DEFAULT_BASE_URL`. The
## control plane is not necessarily the port the room server listens on, so a
## title that ships to both sets `CYGHttp.base_url` on purpose.
static func resolve_base_url() -> String:
	var configured := base_url
	if configured.is_empty() and settings != null:
		configured = settings.server_http_url
	if configured.is_empty():
		configured = _autoload_http_url()
	if configured.is_empty():
		configured = DEFAULT_BASE_URL
	return configured.trim_suffix("/")


## The `CYGNetwork` autoload's `settings.server_http_url`, read through
## `Object.get()` so this layer does not have to name an autoload it may not
## have: a headless tool or a dedicated BaaS build runs without one.
static func _autoload_http_url() -> String:
	var network := _autoload_node()
	if network == null:
		return ""
	var resource := network.get("settings") as CYGServerSettings
	return resource.server_http_url if resource != null else ""


## Joins base and path the way the Unity SDK does: no trailing slash on one,
## exactly one leading slash on the other.
static func resolve_url(path: String) -> String:
	var base := resolve_base_url()
	if base.is_empty():
		return ""
	var suffix := path if path.begins_with("/") else "/" + path
	return base + suffix


## ------------------------------------------------------------------- plumbing

static func _method_code(verb: String) -> int:
	match verb.to_upper():
		"GET":
			return HTTPClient.METHOD_GET
		"HEAD":
			return HTTPClient.METHOD_HEAD
		"POST":
			return HTTPClient.METHOD_POST
		"PUT":
			return HTTPClient.METHOD_PUT
		"DELETE":
			return HTTPClient.METHOD_DELETE
		"OPTIONS":
			return HTTPClient.METHOD_OPTIONS
		"PATCH":
			return HTTPClient.METHOD_PATCH
		_:
			push_warning("CYGHttp: unknown verb '%s'; sending it as POST." % verb)
			return HTTPClient.METHOD_POST


## The `HTTPRequest.Result` name behind a transport failure, since the numeric
## value means nothing to whoever reads a log line.
static func _result_name(result: int) -> String:
	match result:
		HTTPRequest.Result.RESULT_CHUNKED_BODY_SIZE_MISMATCH:
			return "chunked body size mismatch"
		HTTPRequest.Result.RESULT_CANT_CONNECT:
			return "could not connect"
		HTTPRequest.Result.RESULT_CANT_RESOLVE:
			return "could not resolve the host"
		HTTPRequest.Result.RESULT_CONNECTION_ERROR:
			return "connection error"
		HTTPRequest.Result.RESULT_TLS_HANDSHAKE_ERROR:
			return "TLS handshake failed"
		HTTPRequest.Result.RESULT_NO_RESPONSE:
			return "no response"
		HTTPRequest.Result.RESULT_BODY_SIZE_LIMIT_EXCEEDED:
			return "body exceeded the size limit"
		HTTPRequest.Result.RESULT_BODY_DECOMPRESS_FAILED:
			return "body decompression failed"
		HTTPRequest.Result.RESULT_REQUEST_FAILED:
			return "request failed"
		HTTPRequest.Result.RESULT_DOWNLOAD_FILE_CANT_OPEN:
			return "download file could not be opened"
		HTTPRequest.Result.RESULT_DOWNLOAD_FILE_WRITE_ERROR:
			return "download file could not be written"
		HTTPRequest.Result.RESULT_REDIRECT_LIMIT_REACHED:
			return "redirect limit reached"
		HTTPRequest.Result.RESULT_TIMEOUT:
			return "timed out after %.1fs" % timeout_seconds
		_:
			return "request failed (%d)" % result


static func _host_node() -> Node:
	if host != null and is_instance_valid(host) and host.is_inside_tree():
		return host
	var network := _autoload_node()
	if network != null:
		return network
	var tree := Engine.get_main_loop() as SceneTree
	return tree.root if tree != null else null


static func _autoload_node() -> Node:
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null or tree.root == null:
		return null
	return tree.root.get_node_or_null(^"CYGNetwork")


## Frees a finished `HTTPRequest` and drops the reference that was keeping it
## visible to `cancel_all()`.
static func _retire(http: HTTPRequest) -> void:
	_active.erase(http)
	if is_instance_valid(http):
		http.queue_free()


## Aborts every in-flight request without waiting for a reply. A scene change or
## a logout calls it so no callback lands on a node that is already gone.
static func cancel_all() -> void:
	for entry in _active:
		var http := entry as HTTPRequest
		if http != null and is_instance_valid(http):
			http.cancel_request()
	_active.clear()


## Yields the retry pause without blocking the thread. Falling back to the next
## frame when there is no `SceneTree` keeps the call honest in a headless tool.
static func _sleep(seconds: float) -> void:
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		return
	await tree.create_timer(maxf(seconds, 0.0)).timeout
