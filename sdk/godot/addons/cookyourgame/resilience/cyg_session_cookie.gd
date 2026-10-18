class_name CYGSessionCookie
extends RefCounted
## Opaque, locally persisted client identity stored under `user://`.
##
## What this is: a stable id for *this install of this game on this machine*,
## written once and re-read after the process exits. It is the Godot counterpart
## of Unity's `CYGSessionCookie`: the value is a random token the SDK never
## parses, so a game can carry it into a lobby, a ban list, or an analytics
## event without the transport having to understand it.
##
## What this is NOT — stated plainly because the name invites the wrong idea.
## The only seat-resume handshake this transport has is
## `CYGNetwork.reconnect_room()`, which replays the *server-issued*
## `reconnectionToken` that `CYGClient` holds in memory for the seat it was
## matched into (`CYGClient.reconnect_to_room()` posts it to the `reconnect`
## matchmake method). That token is not written to disk here and is not valid
## once the room process reaps the seat. So:
##   * a cookie read in the same process can accompany such a retry;
##   * a cookie read after a restart identifies a returning client and can
##     re-join the same room by name, but it cannot resume the old seat, its
##     actor number, or any of its view state.
## `pending_reconnect` and friends record *that a retry is worth attempting* —
## the server decides whether it succeeds, and a rejected attempt is a fresh
## join with a fresh actor number.
##
## On-disk layout, one `key=value` line per field, written whole on every save:
##   cyg_cookie_version=1
##   value=<64 hex chars>
##   pending_reconnect=0|1
##   reconnect_attempts=<u32>
##   updated_unix=<seconds>
## Unrecognised lines are ignored on read, so a newer file stays readable here.
## There is no wire traffic of any kind in this file.

const VERSION_KEY := "cyg_cookie_version"
const VERSION_VALUE := 1
const VALUE_KEY := "value"
const PENDING_KEY := "pending_reconnect"
const ATTEMPTS_KEY := "reconnect_attempts"
const UPDATED_KEY := "updated_unix"

## Random bytes behind the hex token; 32 bytes is far past what an id needs and
## still costs one line of text.
const TOKEN_BYTES := 32
const HEX_DIGITS := "0123456789abcdef"
## A room can only be re-joined by name; these keys name the file and the field
## a game stores that name under.
const DEFAULT_FILE_NAME := "cyg_session_cookie.cfg"
## Retries this cookie will suggest before telling the caller to give up.
const MAX_RECONNECT_ATTEMPTS := 3

## Name under `user://`; assign before the first `load_or_create()`.
var file_name: String = DEFAULT_FILE_NAME
## Refuse to suggest a retry after this many attempts.
var max_reconnect_attempts: int = MAX_RECONNECT_ATTEMPTS
## Last failure, as Godot's `error_string()` plus the offending path.
var last_error: String = ""

var _value: String = ""
var _pending_reconnect := false
var _reconnect_attempts := 0
var _loaded := false


## The token itself; "" before a successful `load_or_create()` or `generate()`.
var value: String:
	get:
		return _value


## True while an interrupted session is worth retrying. Cleared by
## `resolve_reconnect()` on a successful join and by `clear()`.
var pending_reconnect: bool:
	get:
		return _pending_reconnect


var reconnect_attempts: int:
	get:
		return _reconnect_attempts


func _to_string() -> String:
	return "[CYGSessionCookie %s pending=%s attempts=%d]" % [
		_value.left(8), _pending_reconnect, _reconnect_attempts
	]


## -------------------------------------------------------------------- persistence

## Reads the file, creating and saving a fresh token when it is missing, blank or
## unreadable. Returns whether a usable `value` is in memory afterwards, which is
## true even when the write failed: a game can still play, it just will not be
## recognised next run.
func load_or_create() -> bool:
	last_error = ""
	if not _read():
		generate()
		# A first run that cannot write is still a first run: keep the token for
		# this session rather than regenerating it on every frame of boot.
		save()
		return not _value.is_empty()
	_loaded = true
	return not _value.is_empty()


## Writes the cookie, minting a token first if none is in memory.
func save() -> bool:
	if _value.is_empty():
		generate()
	var lines := {
		VERSION_KEY: str(VERSION_VALUE),
		VALUE_KEY: _value,
		PENDING_KEY: "1" if _pending_reconnect else "0",
		ATTEMPTS_KEY: str(_reconnect_attempts),
		UPDATED_KEY: str(Time.get_unix_time_from_system()),
	}
	var text := ""
	for key in lines.keys():
		text += "%s=%s\n" % [key, lines[key]]
	var path := user_path()
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		last_error = "%s (%s)" % [error_string(FileAccess.get_open_error()), path]
		push_warning("CYGSessionCookie: could not write the cookie: %s" % last_error)
		return false
	file.store_string(text)
	file.close()
	_loaded = true
	return true


## Forgets the client: the in-memory token goes blank and the file is removed.
## Returns whether the file is gone, so a caller on read-only storage can tell.
func clear() -> bool:
	_value = ""
	_pending_reconnect = false
	_reconnect_attempts = 0
	var path := user_path()
	if not FileAccess.file_exists(path):
		return true
	var err := DirAccess.remove_absolute(ProjectSettings.globalize_path(path))
	if err != OK:
		last_error = "%s (%s)" % [error_string(err), path]
		push_warning("CYGSessionCookie: could not remove the cookie: %s" % last_error)
	# The file may still be gone (a race with the OS), so the answer is the state
	# of the disk, not the error code.
	return not FileAccess.file_exists(path)


## The absolute `user://` path this cookie reads and writes.
func user_path() -> String:
	return "user://" + (file_name if not file_name.is_empty() else DEFAULT_FILE_NAME)


## True when a cookie was read from, or written to, disk by this instance.
var loaded_from_disk: bool:
	get:
		return _loaded


## ----------------------------------------------------------------------- the token

## Replaces the in-memory token with a fresh random one; call `save()` to keep it.
func generate() -> String:
	var random := Crypto.new().generate_random_bytes(TOKEN_BYTES)
	if random.size() != TOKEN_BYTES:
		# No entropy source (a sandbox without crypto): fall back to a value that is
		# still unique per run and honest about being machine-local.
		push_warning("CYGSessionCookie: the entropy source returned no bytes; using a machine-derived token.")
		random = ("%s|%d|%d" % [OS.get_unique_id(), Time.get_unix_time_from_system(), Time.get_ticks_usec()]).to_utf8_buffer()
	var text := ""
	for i in range(random.size()):
		var byte := int(random[i])
		text += HEX_DIGITS[(byte >> 4) & 0x0F]
		text += HEX_DIGITS[byte & 0x0F]
	_value = text
	return _value


## ------------------------------------------------------------------ reconnect hints

## Records that the session dropped in a way worth retrying. `code` is the RFC
## 6455 close code the socket reported; the SDK's `CYGOpcodes.CloseMayTryReconnect`
## (4010) is the server saying a retry is allowed, and any other code except a
## consented leave still leaves the choice to the game.
func note_disconnect(reason: String, code: int = 0) -> void:
	last_error = reason
	if code == CYGOpcodes.CloseConsented:
		_pending_reconnect = false
		_reconnect_attempts = 0
		return
	_pending_reconnect = true
	save()


## Consumes the hint: false once the attempt budget is spent, so a game that
## retries on a timer stops rather than hammering a closed room.
func should_attempt_reconnect() -> bool:
	return _pending_reconnect and _reconnect_attempts < max_reconnect_attempts


## Takes one attempt off the budget; call it immediately before the retry.
func begin_reconnect_attempt() -> bool:
	if not should_attempt_reconnect():
		return false
	_reconnect_attempts += 1
	save()
	return true


## Ends the retry cycle: a joined room, or a decision to stop trying.
func resolve_reconnect() -> void:
	_pending_reconnect = false
	_reconnect_attempts = 0
	save()


## ------------------------------------------------------------------------ internals

## Reads the file into memory. False means "no usable cookie on disk", which
## covers a missing file, an unreadable one, and a version this build cannot use.
func _read() -> bool:
	var path := user_path()
	if not FileAccess.file_exists(path):
		last_error = ""
		return false
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		last_error = "%s (%s)" % [error_string(FileAccess.get_open_error()), path]
		push_warning("CYGSessionCookie: could not read the cookie: %s" % last_error)
		return false
	# `get_as_text()` and not `get_csv_line()`: that one call reads a single line,
	# which would leave every field behind the first.
	var fields := _parse_lines(file.get_as_text().split("\n"))
	file.close()
	if str(fields.get(VERSION_KEY, "")) != str(VERSION_VALUE):
		last_error = "unsupported cookie version"
		return false
	var token := str(fields.get(VALUE_KEY, ""))
	# A token of the wrong shape is a truncated or hand-corrupted file; minting a
	# new one is better than letting a game key a player off a half-written id.
	if token.is_empty() or token.length() < TOKEN_BYTES * 2:
		last_error = "cookie value is missing or truncated"
		return false
	_value = token
	_pending_reconnect = str(fields.get(PENDING_KEY, "0")) == "1"
	_reconnect_attempts = int(fields.get(ATTEMPTS_KEY, "0"))
	return true


## Splits the whole file on newlines and keeps only `key=value` pairs.
func _parse_lines(lines: PackedStringArray) -> Dictionary:
	var fields := {}
	for line in lines:
		var text := line.strip_edges()
		var split := text.find("=")
		if split <= 0:
			continue
		fields[text.substr(0, split)] = text.substr(split + 1)
	return fields
