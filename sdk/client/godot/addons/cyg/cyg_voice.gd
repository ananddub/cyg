class_name CYGVoice
extends RefCounted
## Relay-only port of the JS `CYGVoiceMesh`.
##
## The JS mesh negotiates a full WebRTC peer connection per speaker; Godot's
## native `WebRTCPeerConnection` is unavailable on most export targets, so this
## class keeps the same public surface while every speaker stays on the
## authoritative server relay (`CYGRoom.send_voice`). Signaling is still
## parsed: peer roster, mute verdicts and server config update the local view,
## and `is_p2p()` simply always reports false.

signal peer_joined(peer_id: int, mode: String)
signal peer_left(peer_id: int, reason: String)
signal mode_changed(peer_id: int, mode: String)
signal config_received(config: Dictionary)
signal signal_received(frame: Dictionary)
signal error_raised(info: Dictionary)

var room: Variant
var options: Dictionary
var self_id: Variant = null
var ice_servers: Array = []
var peer_timeout_ms := 12000
var data_channels := true

var mic_stream: Variant = null
var mic_track: Variant = null
var owns_mic := false
var enabled := false
var disposed := false

## peer id -> `{ "mode": "relay" }`
var peers: Dictionary = {}
var audio_context: Variant = null

var _unwired: Array = []
var _server_muted: Dictionary = {}
var _listeners: Dictionary = {}


func _init(room_ref: Variant = null, voice_options: Dictionary = {}) -> void:
	room = room_ref
	options = voice_options
	self_id = float(options["selfId"]) if options.has("selfId") and options["selfId"] != null else null
	if options.get("iceServers") is Array:
		ice_servers = (options["iceServers"] as Array).duplicate()
	peer_timeout_ms = int(options.get("peerTimeoutMs", 12000))
	data_channels = options.get("dataChannels", true) != false
	audio_context = options.get("audioContext")


## The relay path never needs a peer-connection implementation.
static func is_supported() -> bool:
	return false


## Subscribe by JS event name (`peer`, `peerleft`, `mode`, `config`, `error`, ...).
func on(event: String, handler: Callable) -> Variant:
	if not _listeners.has(event):
		_listeners[event] = []
	(_listeners[event] as Array).append(handler)
	return self


func off(event: String, handler: Callable) -> Variant:
	var list: Array = _listeners.get(event, [])
	list.erase(handler)
	return self


func _emit(event: String, args: Array = []) -> void:
	var list: Array = _listeners.get(event, [])
	for handler in list.duplicate():
		if handler is Callable and (handler as Callable).is_valid():
			(handler as Callable).callv(args)


## Announce the session to the server voice router. Always relay-only here.
func enable(stream: Variant = null) -> bool:
	if disposed:
		_emit("error", [{ "reason": "voice mesh is disposed" }])
		return false
	if enabled:
		return true
	if stream != null:
		mic_stream = stream
		owns_mic = false
		mic_track = _first_audio_track(stream)
	enabled = true
	var sent := false
	if room != null and room.has_method("send_signal"):
		sent = bool(room.send_signal(
			CYGCodec.SignalKind.JOIN,
			CYGCodec.SIGNAL_BROADCAST,
			{
				"version": CYGCodec.SIGNAL_VERSION,
				"capabilities": "relay,opus" if mic_track != null else "relay",
			}
		))
	if not sent:
		enabled = false
		_emit("error", [{ "reason": "room socket is not open" }])
		return false
	_flush_unwired()
	return true


func set_mic_stream(stream: Variant) -> void:
	mic_stream = stream
	mic_track = _first_audio_track(stream)
	if not enabled:
		enable(stream)


## The server assigns entity ids from game state; the app reports its own id
## once the first delta lands, and queued peers are wired from here.
func set_self_id(id: Variant) -> void:
	if id == null:
		return
	var numeric := float(id)
	if is_nan(numeric) or is_inf(numeric):
		return
	if self_id != null and float(self_id) == numeric:
		return
	self_id = numeric
	_flush_unwired()


func _flush_unwired() -> void:
	if self_id == null or not enabled or _unwired.is_empty():
		return
	var queued := _unwired.duplicate()
	_unwired.clear()
	for id in queued:
		_ensure_peer(id)


func _ensure_peer(peer_id: Variant, _silent: bool = false) -> Dictionary:
	if disposed or peer_id == null:
		return {}
	var id := float(peer_id)
	if is_nan(id) or is_inf(id) or (self_id != null and float(self_id) == id):
		return {}
	if peers.has(id):
		return peers[id]
	peers[id] = { "mode": "relay" }
	peer_joined.emit(int(id), "relay")
	_emit("peer", [int(id), "relay"])
	return peers[id]


func _teardown(peer_id: Variant, reason: String = "left") -> void:
	var id := float(peer_id)
	if not peers.has(id):
		return
	peers.erase(id)
	_server_muted.erase(id)
	peer_left.emit(int(id), reason)
	_emit("peerleft", [int(id), reason])
	_emit("mode", [int(id), "relay"])


## Hand one decoded signal frame to the mesh (called by the room).
func _on_signal(frame: Dictionary) -> void:
	if disposed or frame.is_empty():
		return
	var kind := int(frame.get("kind", 0))
	var body: Variant = frame.get("body")
	match kind:
		CYGCodec.SignalKind.CONFIG:
			if body is Dictionary:
				var config: Dictionary = body
				if config.has("self"):
					set_self_id(config["self"])
				if ice_servers.is_empty() and config.get("iceServers") is Array:
					ice_servers = (config["iceServers"] as Array).duplicate()
				if config.get("peers") is Array:
					for id in config["peers"]:
						_ensure_peer(id)
				config_received.emit(config)
				_emit("config", [config])
		CYGCodec.SignalKind.PEER_JOINED:
			_ensure_peer(_body_peer(body))
		CYGCodec.SignalKind.PEER_LEFT:
			_teardown(_body_peer(body), "left")
		CYGCodec.SignalKind.MUTED:
			if body is Array and (body as Array).size() >= 2:
				_apply_server_mute((body as Array)[0], bool((body as Array)[1]))
		CYGCodec.SignalKind.ERROR:
			var info := { "reason": "rejected" }
			if body is Dictionary and (body as Dictionary).has("reason"):
				info["reason"] = (body as Dictionary)["reason"]
			error_raised.emit(info)
			_emit("error", [info])
		_:
			signal_received.emit(frame)
			_emit("signal", [frame])


func _body_peer(body: Variant) -> Variant:
	if body is Array:
		return (body as Array)[0] if not (body as Array).is_empty() else null
	if body is Dictionary:
		return (body as Dictionary).get("peer")
	return body


func _apply_server_mute(peer_id: Variant, muted: bool) -> void:
	var id := float(peer_id)
	if muted:
		_server_muted[id] = true
	else:
		_server_muted.erase(id)
	set_peer_audio(id, 1.0, 0.0)


## Relay mix is adjusted by the game's own audio bus; kept for API parity.
func set_peer_audio(_peer_id: Variant, _volume: Variant = 1.0, _pan: Variant = 0.0) -> void:
	pass


## Stop transmitting on the local relay path.
func set_muted(muted: bool) -> void:
	if mic_track != null and mic_track is Object and "enabled" in mic_track:
		mic_track.set("enabled", not muted)


## Relay-only: never true.
func is_p2p(_peer_id: Variant) -> bool:
	return false


func should_relay(_peer_id: Variant) -> bool:
	return true


func peer_ids() -> Array:
	return peers.keys()


## No data channel exists on the relay path.
func is_mesh_ready(_peer_id: Variant) -> bool:
	return false


func send_input(_input: Dictionary) -> int:
	return 0


func send_state(_tick: int, _entities: Array) -> int:
	return 0


func send_event(_event: Dictionary) -> int:
	return 0


func send_voice_packet(_packet: PackedByteArray) -> int:
	return 0


## Leave the mesh without dropping the game connection.
func disable() -> void:
	if enabled and room != null and room.has_method("send_signal"):
		room.send_signal(
			CYGCodec.SignalKind.BYE,
			CYGCodec.SIGNAL_BROADCAST,
			{ "version": CYGCodec.SIGNAL_VERSION }
		)
	enabled = false
	for id in peers.keys():
		_teardown(id, "left")
	_unwired.clear()
	_release_mic()


func dispose() -> void:
	if disposed:
		return
	disposed = true
	disable()
	_listeners.clear()


func _release_mic() -> void:
	if owns_mic and mic_stream != null and mic_stream is Object and mic_stream.has_method("get_tracks"):
		for track in mic_stream.call("get_tracks"):
			if track != null and track.has_method("stop"):
				track.call("stop")
	mic_stream = null
	mic_track = null
	owns_mic = false


func _first_audio_track(stream: Variant) -> Variant:
	if stream == null or not (stream is Object):
		return null
	if not stream.has_method("get_audio_tracks"):
		return null
	var tracks: Array = stream.call("get_audio_tracks")
	return tracks[0] if not tracks.is_empty() else null
