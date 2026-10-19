class_name CYGAuth
extends RefCounted
## Player authentication and session lifetime for the CookYourGame BaaS layer.
##
## One line signs a guest in, and the token it returns is what every other BaaS
## call carries:
##
##     CYGAuth.authenticated.connect(_on_signed_in)
##     CYGAuth.login_device()                       # fire and forget
##
##     # or, from a node script, when the answer is needed before proceeding
##     var session: CYGAuth.Session = await CYGAuth.login_device()
##     if session == null:
##         print(CYGAuth.last_error)
##
## Endpoints, bodies and replies, exactly as
## `crates/engine-api/src/controllers/auth.rs` and `session.rs` bind them:
##
##   POST `/v2/account/authenticate/device?create=<bool>[&username=<text>]`
##        body `{"id": "<device id>"}`                — dto `auth::IdAccount`
##   POST `/v2/account/authenticate/email?create=…`
##        body `{"email": "…", "password": "…"}`      — dto `auth::EmailAccount`
##   POST `/v2/account/authenticate/custom?create=…`
##        body `{"id": "<caller-chosen id>"}`         — dto `auth::IdAccount`
##   POST `/v2/account/session/refresh`  body `{"token": "<refresh token>"}`
##   POST `/v2/session/logout`           body `{"token": "…", "refresh_token": "…"}`
##
## `create` and `username` are query parameters and never body fields: the
## gateway's `AuthQuery` reads them from the query and `BoolFlag::parse` accepts
## only `1|0|t|f|T|F|true|false|True|False|TRUE|FALSE` in exact spelling, so this
## SDK always writes the lowercase `true`/`false`. An absent `create` means
## create-on-miss to the engine, which is why the flag is always sent rather
## than omitted when it is true.
##
## The reply to all three authenticate routes and to `refresh` is
## `crate::dto::session::SessionResponse`:
##
##   `token`          — the bearer token, HS256, `control.session.token_expiry_secs`
##   `refresh_token`  — the refresh-half of the same pair (`tid`, `iat` shared)
##   `created`        — omitted entirely when false, since the marshal drops
##                      default values; absent therefore means "existing account"
##
## There is no `userId`, `username` or `deviceId` in that reply. Identity comes
## out of the token itself: `crate::infra::jwt::Claims` serialises the user id
## as `uid` and the nickname as `usn` (`#[serde(rename = "usn")]` on a field
## named `un`), so `user_id` and `username` below are read from the JWT payload,
## exactly as the Unity SDK's `ProcessAuthResponse` and the Unreal SDK's
## `ParseClaims` do. A `user` member in the body is used only as the fallback
## the Unreal port already reaches for, in case a future plane version sends one.
##
## Credentials split by route, not by preference: `Authenticate*` and
## `SessionRefresh` demand the server key and refuse anything else, so they are
## sent with `require_auth = false` and `CYGHttp` answers that with
## `Authorization: Basic base64("<server_key>:")`. `/v2/session/logout` is a
## bearer route. See `cyg_http.gd` for why the two never go out together.
##
## The session survives a restart the way Unity's `PlayerPrefs` does, through
## `FileAccess` under `user://`, in the same line-oriented layout this SDK's
## `CYGSessionCookie` uses — one `key=value` per line, a version key first, and
## unrecognised lines ignored on read so a newer file stays readable here:
##
##   cyg_session_version=1
##   token=<jwt>
##   refresh_token=<jwt>
##   user_id=<uuid text>
##   username=<text, newlines replaced>
##   device_id=<32 hex chars>
##   updated_unix=<seconds>
##
## `device_id` is written whether or not the login itself was persisted: it is
## not a credential, and a guest that signs out must come back as the same
## guest. `logout()` clears the token fields and leaves the device id alone.
##
## Independent GDScript port of this repo's own SDKs (`sdk/unity/Runtime/BaaS/
## CYGAuth.cs` and `sdk/unreal/.../BaaS/CYGBaasAuth.cpp`); no upstream source
## text is copied. Nothing here derives from Netfox — the BaaS layer has no
## Netfox lineage, so this file carries no LICENSE-NETFOX.md attribution.

## Field names as the engine's own DTOs spell them: `SessionResponse` is
## `#[derive(Serialize)]` with no rename attribute, so the snake case is the
## wire spelling, and `created` may be missing entirely.
const FIELD_TOKEN := "token"
const FIELD_REFRESH_TOKEN := "refresh_token"
const FIELD_CREATED := "created"
## JWT claim names, per `crate::infra::jwt::Claims`.
const CLAIM_USER_ID := "uid"
const CLAIM_USERNAME := "usn"

const AUTHENTICATE_PATH := "/v2/account/authenticate/"
const REFRESH_PATH := "/v2/account/session/refresh"
const LOGOUT_PATH := "/v2/session/logout"

## On-disk session file, sibling of `cyg_session_cookie.cfg`.
const DEFAULT_FILE_NAME := "cyg_auth_session.cfg"
const VERSION_KEY := "cyg_session_version"
const VERSION_VALUE := 1
const TOKEN_KEY := "token"
const REFRESH_KEY := "refresh_token"
const USER_ID_KEY := "user_id"
const USERNAME_KEY := "username"
const DEVICE_ID_KEY := "device_id"
const UPDATED_KEY := "updated_unix"
## Bytes behind the generated device id when the platform offers no stable one;
## 16 renders as 32 hex characters, the shape of Unity's `Guid("N")`.
const DEVICE_ID_BYTES := 16
const HEX_DIGITS := "0123456789abcdef"

## Signed in with a live bearer token. The engine's routes say so themselves:
## `token_expiry_secs` defaults to 60, so a long lobby session is refreshed, not
## re-logged-in; `refresh_session()` is the call that does it.
static var session_token: String = ""
static var refresh_token: String = ""
## Identity read out of the token's claims, never guessed from the request.
static var user_id: String = ""
static var username: String = ""
## The id `login_device()` authenticated with, and the one it reuses after a
## restart. Assigned by `resolve_device_id()`; set it directly when the platform
## has a better identifier than `OS.get_unique_id()`.
static var device_id: String = ""
## False in a build that must not leave a session on disk (an editor preview, a
## kiosk that hands the machine to the next player).
static var persist_session: bool = true
## Name under `user://`; assign before the first read or write.
static var file_name: String = DEFAULT_FILE_NAME
## Why the last call failed, as the engine said it (`error.message`) or as the
## transport did. "" when nothing has failed.
static var last_error: String = ""
## `error.code` from the last failure envelope: 16 unauthenticated, 3 invalid
## argument, 6 already exists, 5 not found. 0 when the failure was local.
static var last_error_code: int = 0
## Emits `authenticated(session)` and `logged_out()`. A `RefCounted` signal bus
## rather than signals on this class, because these are static entry points and
## GDScript signals need an instance to emit from.
static var events: Events = Events.new()

static var _restored := false


## The reply, held in memory so a caller can read `created` without reparsing
## the body, and printed without guessing what a `Dictionary` key was called.
class Session extends RefCounted:
	## True when this call made the account, false when it signed into one that
	## was already there. The reply omits the field on false.
	var created: bool = false
	## Bearer token; the only field a session cannot exist without.
	var token: String = ""
	## The refresh half of the pair, minted under the same `tid`.
	var refresh_token: String = ""
	## `uid` from the token claims: the UUID text the engine stores the account under.
	var user_id: String = ""
	## `usn` from the token claims.
	var username: String = ""
	## The id this session was authenticated with — device, custom or the email
	## address — so a game can show or re-use it without keeping its own copy.
	var account_id: String = ""
	## The whole reply object, for a field this class has not named yet.
	var raw: Dictionary = {}

	func _to_string() -> String:
		return "[CYGAuth.Session user=%s name=%s created=%s token=%d chars]" % [
			user_id.left(8), username, created, token.length()
		]


## Signal carrier for `CYGAuth.events`.
class Events extends RefCounted:
	## Fires after the token is stored and published to `CYGHttp`, with the
	## session already populated. `null` is never passed.
	signal authenticated(session)
	## Fires after the local session is gone and `CYGHttp` has no bearer to send.
	signal logged_out()


## ------------------------------------------------------------------- sign in

## Guest one-click login. With no argument this authenticates the machine's
## own id, creating the account on first run and signing into it afterwards.
static func login_device(on_done: Callable = Callable(), device: String = "",
		create: bool = true, display_name: String = "") -> Session:
	_ensure_restored()
	var account := device if not device.is_empty() else resolve_device_id()
	return await _login("device", {"id": account}, account, create, display_name, on_done)


## Email and password. `create` defaults to false here, as the Unity SDK has it:
## a password login that silently mints an account is a support ticket.
static func login_email(on_done: Callable = Callable(), email: String = "", password: String = "",
		create: bool = false, display_name: String = "") -> Session:
	_ensure_restored()
	if email.is_empty() or password.is_empty():
		return _fail("login_email needs an email and a password", "email", true, on_done)
	return await _login("email", {"email": email, "password": password}, email, create, display_name, on_done)


## A caller-chosen id: a save-file slot, a platform account, the game's own user
## key. Anything stable and secret enough to be worth calling an account.
static func login_custom(on_done: Callable = Callable(), custom_id: String = "",
		create: bool = true, display_name: String = "") -> Session:
	_ensure_restored()
	if custom_id.is_empty():
		return _fail("login_custom needs an id", "custom", true, on_done)
	return await _login("custom", {"id": custom_id}, custom_id, create, display_name, on_done)


## The shared shape of the three: build the query, POST the body without a
## bearer (the plane proves the caller with the server key), hand the reply to
## `process_auth_response`.
static func _login(method: String, body: Dictionary, account: String,
		create: bool, display_name: String, on_done: Callable) -> Session:
	var reply := await CYGHttp.send(
		"POST", login_path(method, create, display_name), JSON.stringify(body), false)
	var session := process_auth_response(reply, method, account)
	if on_done.is_valid():
		on_done.call(session)
	return session


## Assembles `?create=&username=`. `uri_encode()` on the name is the counterpart
## of `UnityWebRequest.EscapeURL`: a nickname with a space or an `&` in it must
## not become two query parameters.
static func login_path(method: String, create: bool, display_name: String) -> String:
	var path := AUTHENTICATE_PATH + method + "?create=" + ("true" if create else "false")
	if not display_name.is_empty():
		path += "&username=" + display_name.uri_encode()
	return path


## ------------------------------------------------------------------- refresh

## Trades the stored refresh token for a fresh pair. The engine re-issues under
## the same `tid` and `iat`, so a token family keeps one revocation handle and a
## logout that blacklisted it still covers the token this call just minted.
static func refresh_session(on_done: Callable = Callable()) -> Session:
	_ensure_restored()
	if refresh_token.is_empty():
		return _fail("no refresh token is stored; log in instead", "refresh", true, on_done)

	var body := JSON.stringify({FIELD_TOKEN: refresh_token})
	var reply := await CYGHttp.send("POST", REFRESH_PATH, body, false)
	var session := process_auth_response(reply, "refresh", "")
	if on_done.is_valid():
		on_done.call(session)
	return session


## -------------------------------------------------------------------- logout

## Drops the session locally, then — unless asked not to — tells the plane to
## blacklist the pair it just forgot.
##
## The local clear is first and unconditional on purpose: a player who presses
## sign out is signed out even when the control plane is unreachable, and
## `logout()` in the Unity SDK is local-only, so no game can be surprised by a
## slower answer here than there. The revocation then runs on its own, and its
## outcome only ever becomes a warning: by then the client has forgotten the
## token, and the only thing left to say is that an old one may still live until
## its `exp`.
##
## One window is worth naming: while the revocation is in flight, `CYGHttp`
## briefly holds the token that was just forgotten, because that call has to
## present it. `CYGAuth.is_authenticated()` is already false throughout, so the
## game knows the truth; a BaaS call made inside the window carries a bearer the
## plane has not blacklisted yet and will be refused once it has. `_revoke` hands
## the transport back whatever `CYGAuth` holds when it finishes, so a login that
## wins the race keeps its own token rather than losing it to the old one.
static func logout(revoke_on_server: bool = true) -> void:
	_ensure_restored()
	var token := session_token
	var refresh := refresh_token
	clear_session()
	events.logged_out.emit()
	if revoke_on_server and not token.is_empty():
		_revoke({"token": token, FIELD_REFRESH_TOKEN: refresh})


## Sends the revocation for a session that is already gone locally. The bearer
## has to be published again for the duration: `_revoke` hands `CYGHttp.send`
## the call, and `CYGHttp` reads `session_token` while it assembles headers,
## which happens before the request suspends. Whatever the player did in the
## meantime wins, so the token goes back to whatever is current on the way out.
static func _revoke(tokens: Dictionary) -> void:
	var token := str(tokens.get("token", ""))
	var refresh := str(tokens.get(FIELD_REFRESH_TOKEN, ""))
	CYGHttp.publish_token(token)
	var body := JSON.stringify({"token": token, FIELD_REFRESH_TOKEN: refresh})
	var reply := await CYGHttp.send("POST", LOGOUT_PATH, body, true)
	CYGHttp.publish_token(session_token)
	if not reply.is_success:
		push_warning("CYGAuth: the plane did not take the logout (%s)." % reply.error_message())


## --------------------------------------------------------------- session state

## A live bearer token is the whole definition of signed in.
static func is_authenticated() -> bool:
	_ensure_restored()
	return not session_token.is_empty()


## Re-attaches a token that is in memory but not on the transport — a build that
## assigned `CYGAuth.session_token` by hand, or a `CYGHttp` that was reset.
static func publish_session() -> void:
	CYGHttp.publish_token(session_token)


## Forgets the session in memory and on disk, keeping the device id. Called by
## `logout()`; a game that wants a clean slate calls both this and `clear()`.
static func clear_session() -> void:
	session_token = ""
	refresh_token = ""
	user_id = ""
	username = ""
	CYGHttp.publish_token("")
	_persist()


## ------------------------------------------------------------- reply handling

## The `ProcessAuthResponse` equivalent: read the pair, take the identity out of
## the token rather than the request, store, publish, announce. Returns null and
## fills `last_error` when the reply carries no token, which is every refusal
## the engine makes — and is reported as a warning, not an error, because a 409
## from a taken nickname is an answer a game shows in a dialog box.
static func process_auth_response(reply: CYGHttp.Response, method: String, account: String) -> Session:
	var session := Session.new()
	session.account_id = account
	if reply == null:
		return _fail("the call produced no reply", method, true)
	last_error_code = reply.error_code()
	if not reply.is_success:
		# The plane refused and said why, in `error.message` at the status
		# `ErrorKind` chose. A refusal is an answer rather than an incident: a
		# 409 from a taken nickname or a 401 from a wrong password is a dialog box
		# in a finished game, so nothing is pushed to the console. Only a reply
		# that never arrived is worth a warning, because that one the caller
		# cannot tell apart from a refusal by reading `last_error_code`.
		return _fail(reply.error_message(), method, not reply.reached_server)

	var body: Variant = reply.json
	if typeof(body) != TYPE_DICTIONARY:
		return _fail("the %s call answered with something that is not a JSON object: %s"
			% [method, reply.text.left(160)], method, true)

	var object := body as Dictionary
	session.raw = object
	session.token = str(object.get(FIELD_TOKEN, ""))
	session.refresh_token = str(object.get(FIELD_REFRESH_TOKEN, ""))
	# An absent `created` is the engine saying false, not a field it forgot.
	session.created = bool(object.get(FIELD_CREATED, false))

	if session.token.is_empty():
		return _fail("the %s call succeeded but gave no token: %s"
			% [method, str(object.get("error", reply.error_message()))], method, true)

	# The token is the authority on identity; the body only covers for a build of
	# this SDK that cannot decode a claim set it has not seen.
	var claims := decode_claims(session.token)
	session.user_id = str(claims.get(CLAIM_USER_ID, ""))
	session.username = str(claims.get(CLAIM_USERNAME, ""))
	if session.user_id.is_empty():
		var user: Variant = object.get("user")
		if typeof(user) == TYPE_DICTIONARY:
			session.user_id = str(user.get("id", ""))
			if session.username.is_empty():
				session.username = str(user.get("username", ""))

	session_token = session.token
	refresh_token = session.refresh_token
	user_id = session.user_id
	username = session.username
	last_error = ""
	CYGHttp.publish_token(session_token)
	_persist()
	events.authenticated.emit(session)
	return session


## Base64url-decodes the middle of a JWT and returns its claims, `{}` when the
## token is not readable. No `push_error` on the failure path: an expired or
## unusually-shaped token is still a valid bearer until the server says
## otherwise, and the caller has the reply to judge.
static func decode_claims(jwt: String) -> Dictionary:
	var parts := jwt.split(".")
	if parts.size() < 2:
		return {}
	# base64url is the same alphabet with two swapped characters and the padding
	# dropped, so put the padding back and swap them out again.
	var payload := parts[1].replace("-", "+").replace("_", "/")
	while payload.length() % 4 != 0:
		payload += "="
	if not _is_base64(payload):
		push_warning("CYGAuth: a session token was not readable as base64, so its claims were skipped.")
		return {}
	# `Marshalls.base64_to_raw` complains loudly about input that is not base64,
	# which is what the check above keeps it from ever seeing.
	var decoded: Variant = JSON.parse_string(Marshalls.base64_to_raw(payload).get_string_from_utf8())
	if typeof(decoded) == TYPE_DICTIONARY:
		return decoded
	return {}


static func _is_base64(text: String) -> bool:
	for i in range(text.length()):
		var c := text[i]
		if c != "=" and ALPHA.find(c) < 0 and DIGITS.find(c) < 0 and c != "+" and c != "/":
			return false
	return text.length() % 4 == 0


const ALPHA := "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
const DIGITS := "0123456789"


## Records why nothing happened and answers null, so every rejection path in
## this class has one shape. `warn` is set only for a failure the caller cannot
## diagnose from `last_error_code` alone: a missing argument, a plane that never
## answered, a 2xx reply with no token in it.
static func _fail(reason: String, context: String, warn: bool,
		on_done: Callable = Callable()) -> Session:
	last_error = reason
	if warn:
		push_warning("CYGAuth: %s (%s)." % [reason, context if not context.is_empty() else "local"])
	if on_done.is_valid():
		on_done.call(null)
	return null


## ------------------------------------------------------------------ device id

## The id a guest login uses: `OS.get_unique_id()`, or a generated 32-hex
## surrogate when the platform has nothing stable to offer — the counterpart of
## Unity's `SystemInfo.deviceUniqueIdentifier` falling back to
## `Guid.NewGuid().ToString("N")`. A surrogate is remembered, so the same
## install keeps signing in as the same guest across restarts.
static func resolve_device_id() -> String:
	_ensure_restored()
	if not device_id.is_empty():
		return device_id

	var unique := OS.get_unique_id()
	if _is_usable_id(unique):
		device_id = unique
	else:
		device_id = _generate_device_id()
	# The device id outlives a session, so it is written even when the title asked
	# for its logins not to be.
	if not _persist_device_id():
		push_warning("CYGAuth: the device id could not be written; this install will not be recognised next run.")
	return device_id


## An id the engine can act on: not blank, not a platform stub that reports its
## own unavailability, and not so short that every machine on a network shares it.
static func _is_usable_id(candidate: String) -> bool:
	if candidate.length() < 8:
		return false
	var lowered := candidate.to_lower()
	for stub in ["notimplemented", "unsupported", "unknown", "nil", "none"]:
		if lowered.replace("-", "").replace("_", "").contains(stub):
			return false
	# All zeroes is what a missing machine-id file reads like.
	var all_zero := true
	for i in range(candidate.length()):
		if candidate[i] != "0" and candidate[i] != "-":
			all_zero = false
			break
	return not all_zero


static func _generate_device_id() -> String:
	var random := Crypto.new().generate_random_bytes(DEVICE_ID_BYTES)
	if random.size() != DEVICE_ID_BYTES:
		# No entropy source: fall back to something still unique to this process,
		# and honest about being machine-local rather than account-grade.
		push_warning("CYGAuth: the entropy source returned no bytes; deriving a device id from the clock.")
		random = ("%s|%d|%d" % [
			OS.get_unique_id(), Time.get_unix_time_from_system(), Time.get_ticks_usec()
		]).to_utf8_buffer()
	var text := ""
	for i in range(random.size()):
		var byte := int(random[i])
		text += HEX_DIGITS[(byte >> 4) & 0x0F]
		text += HEX_DIGITS[byte & 0x0F]
	return text


## -------------------------------------------------------------- persistence

## Reads the stored session back and re-publishes it. Returns whether a usable
## bearer is in memory afterwards. Idempotent, and already run by every call
## above, so a game normally never has to name it — except when it wants the
## token before its first frame, where `_ready()` is the honest place.
static func restore_session() -> bool:
	_restored = true
	var fields := _read()
	if fields.is_empty():
		return false
	device_id = str(fields.get(DEVICE_ID_KEY, ""))
	session_token = str(fields.get(TOKEN_KEY, ""))
	refresh_token = str(fields.get(FIELD_REFRESH_TOKEN, ""))
	user_id = str(fields.get(USER_ID_KEY, ""))
	username = str(fields.get(USERNAME_KEY, ""))
	# A token restored without its claims is still the one the server issued, so
	# decode rather than drop the identity a truncated file lost.
	if session_token.is_empty() and not refresh_token.is_empty():
		push_warning("CYGAuth: the stored session has a refresh token and no bearer; it needs a login.")
	elif not session_token.is_empty() and user_id.is_empty():
		var claims := decode_claims(session_token)
		user_id = str(claims.get(CLAIM_USER_ID, ""))
		username = str(claims.get(CLAIM_USERNAME, ""))
	CYGHttp.publish_token(session_token)
	return not session_token.is_empty()


static func _ensure_restored() -> void:
	if not _restored:
		restore_session()


## Writes the session file. `device_id` is written even when `persist_session`
## is false, and the four credential fields are then cleared from disk so a
## build that opted out leaves nothing behind.
static func _persist() -> bool:
	if not persist_session:
		_persist_device_id()
		return false
	var path := user_path()
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		push_warning("CYGAuth: could not write the session: %s (%s)"
			% [error_string(FileAccess.get_open_error()), path])
		return false
	var text := "%s=%d\n" % [VERSION_KEY, VERSION_VALUE]
	text += "%s=%s\n" % [TOKEN_KEY, session_token]
	text += "%s=%s\n" % [FIELD_REFRESH_TOKEN, refresh_token]
	text += "%s=%s\n" % [USER_ID_KEY, user_id]
	text += "%s=%s\n" % [USERNAME_KEY, _sanitize(username)]
	text += "%s=%s\n" % [DEVICE_ID_KEY, device_id]
	text += "%s=%s\n" % [UPDATED_KEY, str(Time.get_unix_time_from_system())]
	file.store_string(text)
	file.close()
	return true


## Writes only the device id, keeping the rest of a file that already exists, so
## a logout or a first guest id never truncates a live session.
static func _persist_device_id() -> bool:
	if device_id.is_empty():
		return false
	var fields := _read()
	fields[VERSION_KEY] = str(VERSION_VALUE)
	fields[DEVICE_ID_KEY] = device_id
	fields[UPDATED_KEY] = str(Time.get_unix_time_from_system())
	if not fields.has(TOKEN_KEY):
		fields[TOKEN_KEY] = session_token
	if not fields.has(FIELD_REFRESH_TOKEN):
		fields[FIELD_REFRESH_TOKEN] = refresh_token
	if not fields.has(USER_ID_KEY):
		fields[USER_ID_KEY] = user_id
	if not fields.has(USERNAME_KEY):
		fields[USERNAME_KEY] = _sanitize(username)
	var path := user_path()
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		push_warning("CYGAuth: could not write the device id: %s (%s)"
			% [error_string(FileAccess.get_open_error()), path])
		return false
	var text := ""
	for key in [VERSION_KEY, TOKEN_KEY, FIELD_REFRESH_TOKEN, USER_ID_KEY, USERNAME_KEY, DEVICE_ID_KEY, UPDATED_KEY]:
		text += "%s=%s\n" % [key, str(fields.get(key, ""))]
	file.store_string(text)
	file.close()
	return true


## Removes the session file and forgets the device id too — the difference
## between signing out and uninstalling.
static func erase_stored_session() -> bool:
	clear_session()
	device_id = ""
	var path := user_path()
	if not FileAccess.file_exists(path):
		return true
	var err := DirAccess.remove_absolute(ProjectSettings.globalize_path(path))
	if err != OK:
		push_warning("CYGAuth: could not remove the session: %s (%s)" % [error_string(err), path])
	return not FileAccess.file_exists(path)


static func user_path() -> String:
	return "user://" + (file_name if not file_name.is_empty() else DEFAULT_FILE_NAME)


## Reads the file into a field map. An empty map means "nothing usable on disk",
## which covers a missing file, an unreadable one, and a version this build
## cannot use.
static func _read() -> Dictionary:
	var path := user_path()
	if not FileAccess.file_exists(path):
		return {}
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		push_warning("CYGAuth: could not read the session: %s (%s)"
			% [error_string(FileAccess.get_open_error()), path])
		return {}
	# `get_as_text()` for the whole file: a single-line read would drop every
	# field behind the first, which is the same trap `CYGSessionCookie` names.
	var text := file.get_as_text()
	file.close()
	var fields := _parse_lines(text.split("\n"))
	if str(fields.get(VERSION_KEY, "")) != str(VERSION_VALUE):
		push_warning("CYGAuth: the stored session is a version this build cannot read.")
		return {}
	return fields


## Splits the whole file on newlines and keeps only `key=value` pairs; the split
## is on the first `=`, which is what lets a padded base64 token round-trip.
static func _parse_lines(lines: PackedStringArray) -> Dictionary:
	var fields := {}
	for line in lines:
		var text := line.strip_edges()
		var split := text.find("=")
		if split <= 0:
			continue
		fields[text.substr(0, split)] = text.substr(split + 1)
	return fields


## A nickname cannot carry a newline, or it becomes two lines of the file.
static func _sanitize(value: String) -> String:
	return value.replace("\r", " ").replace("\n", " ")
