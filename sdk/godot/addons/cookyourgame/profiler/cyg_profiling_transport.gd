class_name CYGProfilingTransport
extends CYGClient
## Instrumenting, degradable wrapper around a live `CYGClient`.
##
## It *is* a `CYGClient` (the base owns no socket of its own here: every verb
## forwards to `inner`), so it can be installed by swapping the public
## `CYGNetwork.client` reference — no core edit, mirroring the Unity SDK's
## transport-factory seam and the Unreal SDK's `UseTransport`. Each outbound
## `send_frame`/`send_wire` and each inbound deframed `frame` is counted into
## the bound `CYGNetworkProfiler` with the same header-stripped byte semantics
## the Unity/Unreal wrappers use, and `StateDelta` traffic is additionally
## attributed to its view by decoding the `[zigzag varint viewId]` prefix.
##
## With `degrade_enabled` it also becomes the lag simulator a developer uses to
## test rollback by hand: packets in both directions gain `latency_ms` ±
## `jitter_ms` of delay, vanish at `drop_percent` and are re-sent at
## `duplicate_percent`. The RNG is a seeded `RandomNumberGenerator`, so a run
## is reproducible. Queued packets are bounded by `MAX_DEGRADE_QUEUE`; when the
## queue is full the oldest packet is force-delivered (never lost), so counts
## stay truthful under a stalled clock.
##
## Cost model: with `degrade_enabled = false` the wrapper adds one branch per
## direction and forwards synchronously — same call, same order, same bytes.
## `detach()` unwinds everything (see `install()`/`detach()`), restoring the
## untouched client. Attaching allocates once: the wrapper, its profiler and
## the signal taps.
##
## Install timing matters for who the wrapper can intercept:
##  * Before `CYGNetwork._ready()`: call `attach()` while `client` is still
##    null (a plugin init or preload script can). The autoload adopts the
##    wrapper at `_ready` and connects its own handlers to it from the start,
##    so every send and receive crosses the wrapper.
##  * After `_ready()`: `attach()` re-routes the autoload's own signal
##    connections from the inner client onto the wrapper and restores them
##    verbatim at `detach()`. Code that had connected directly to the *old*
##    client object outside the autoload keeps seeing its raw events; install
##    early to avoid that blind spot.
##
## Usage:
##
##     var wrapper := CYGProfilingTransport.attach()      # finds the autoload
##     wrapper.profiler.enabled = true
##     wrapper.degrade_enabled = true
##     wrapper.latency_ms = 120.0
##     wrapper.jitter_ms = 30.0
##     wrapper.drop_percent = 5.0
##     # ... play, read CYGNetworkProfiler.active.snapshot() ...
##     wrapper.detach()                                   # byte-identical again
##
## Design derives from Netfox's MIT-licensed profiler and Mirror's interest/transport metrics (attribution in LICENSE-NETFOX.md). Clean-room GDScript; no upstream source text is copied.

## Hard cap on each degrade queue; overflow force-delivers the oldest packet.
const MAX_DEGRADE_QUEUE := 2048

## The wrapped client, kept reachable for teardown and inspection (parity with
## the Unity wrapper's `Inner`).
var inner: CYGClient

## The counting sink this wrapper feeds; created with the wrapper and enabled by
## `install()`.
var profiler: CYGNetworkProfiler

## Master switch for the artificial-degradation path. False means pure
## forwarding with counting.
var degrade_enabled := false

## Simulated one-way delay in milliseconds.
var latency_ms := 50.0

## Half-width of a uniform ± delay perturbation in milliseconds.
var jitter_ms := 0.0

## Chance (0..100) that a packet is silently discarded, per direction.
var drop_percent := 0.0

## Chance (0..100) that a packet is re-delivered once more, per direction.
## A packet hit by the drop roll is never duplicated.
var duplicate_percent := 0.0

## Seed for the degradation RNG; 0 randomizes on `install()`.
var degradation_seed := 0

var dropped_in := 0
var dropped_out := 0
var duplicated_in := 0
var duplicated_out := 0

var _rng := RandomNumberGenerator.new()
var _pending_in: Array = []
var _pending_out: Array = []
var _moved: Array = []
var _tapped: Array = []
var _ping_chain: Callable
var _network: Object = null
var _previous_active: CYGNetworkProfiler = null
var _installed := false
var _detached := false

# Inbound frames carry the canonical header; the reader is reused so per-packet
# view attribution allocates nothing.
var _scratch_reader := CYGBitReader.new()

const _TAP_PLAN := [
	["frame_received", "_on_inner_frame"],
	["socket_opened", "_on_inner_socket_opened"],
	["socket_closed", "_on_inner_socket_closed"],
	["server_error", "_on_inner_server_error"],
	["seat_ready", "_on_inner_seat_ready"],
	["seat_failed", "_on_inner_seat_failed"],
]


func _init(to_inner: CYGClient = null) -> void:
	inner = to_inner if to_inner != null else CYGClient.new()
	profiler = CYGNetworkProfiler.new()
	_ping_chain = Callable(self, "_on_inner_ping_sent")


## --------------------------------------------------------------- installation

## Swaps a `CYGProfilingTransport` in front of `network.client` (the autoload
## when omitted), creating the inner `CYGClient` if the autoload has not built
## one yet. Idempotent: attaching to an already-wrapped network returns the
## existing wrapper. Returns null when no network node is available.
static func attach(network: Object = null) -> CYGProfilingTransport:
	if network == null:
		var loop := Engine.get_main_loop() as SceneTree
		network = loop.root.get_node_or_null("CYGNetwork") if loop != null else null
	if network == null or not ("client" in network):
		push_error("CYGProfilingTransport.attach: no network node with a `client` property.")
		return null
	var current = network.client
	if current is CYGProfilingTransport:
		return current as CYGProfilingTransport
	var wrapper := CYGProfilingTransport.new(current as CYGClient)
	wrapper.install(network)
	return wrapper


## Wires the wrapper under `network`; use `attach()` unless constructing the
## wrapper by hand (tests inject a scripted subclass this way).
func install(network: Object) -> void:
	if _installed:
		return
	_installed = true
	_network = network
	if degradation_seed != 0:
		_rng.seed = degradation_seed
	else:
		_rng.randomize()

	var wired := inner.host == network
	if wired:
		# The autoload already talks to `inner`; take over the properties it
		# assigned and re-route its own signal handlers onto this wrapper.
		host = inner.host
		settings = inner.settings
		if inner.on_ping_sent.is_valid():
			on_ping_sent = inner.on_ping_sent
		_move_connections(network)
	elif inner.host == null:
		inner.host = network

	network.client = self
	_tap_inner_signals()

	profiler.enabled = true
	_previous_active = CYGNetworkProfiler.active
	CYGNetworkProfiler.active = profiler
	profiler.bind_network(network)


## Removes the wrapper: the network sees `inner` again, the autoload's signal
## handlers are reconnected where they were, and the profiler stops being
## pumped. Afterwards behaviour is byte-identical to an unwrapped client.
func detach() -> void:
	if _detached or not _installed:
		return
	_detached = true

	for entry in _tapped:
		var tap_signal: Signal = entry["signal"]
		var tap_handler: Callable = entry["handler"]
		if tap_signal.is_connected(tap_handler):
			tap_signal.disconnect(tap_handler)
	_tapped.clear()

	for entry in _moved:
		var from_wrapper := Signal(self, entry["signal"])
		if from_wrapper.is_connected(entry["callable"]):
			from_wrapper.disconnect(entry["callable"])
		var onto_inner := Signal(inner, entry["signal"])
		if not onto_inner.is_connected(entry["callable"]):
			onto_inner.connect(entry["callable"])
	_moved.clear()

	# Hand the ping-timestamp callback back verbatim: whatever the wrapper's
	# `on_ping_sent` holds (the autoload recorder, or null if the client never
	# had one) becomes the inner's again, unwrapping the counting chain.
	inner.on_ping_sent = on_ping_sent if on_ping_sent.is_valid() else Callable()

	profiler.unbind_network()
	if CYGNetworkProfiler.active == profiler:
		CYGNetworkProfiler.active = _previous_active

	if _network != null and is_instance_valid(_network) and _network.client == self:
		_network.client = inner
	_network = null
	_pending_in.clear()
	_pending_out.clear()


## Clears the degradation counters without touching traffic statistics.
func reset_degradation_stats() -> void:
	dropped_in = 0
	dropped_out = 0
	duplicated_in = 0
	duplicated_out = 0


## ------------------------------------------------------------------ overrides

## Drives the inner client, mirrors its link state and the properties the
## autoload assigns to `client`, feeds the profiler clock and flushes whatever
## the degradation queues have made due.
func poll() -> void:
	_sync_callbacks_to_inner()
	inner.poll()
	is_connected = inner.is_connected
	last_close_code = inner.last_close_code
	var now := _clock_now()
	if profiler != null and profiler.enabled:
		profiler.pump(now)
	if degrade_enabled:
		_flush_due(now)


## Counts one framed send and forwards it, degraded when configured.
func send_frame(op: int, payload: PackedByteArray) -> bool:
	if profiler != null and profiler.enabled:
		profiler.record_send(op, payload.size())
		if op == CYGOpcodes.StateDelta:
			_attribute_outbound_view_delta(payload)
	if not degrade_enabled or not inner.is_connected:
		return inner.send_frame(op, payload)
	if _roll_dropped():
		dropped_out += 1
		return true
	var now := _clock_now()
	var due := now + _delay_seconds()
	_enqueue_outbound(_pending_out, due, false, op, payload)
	if _roll_duplicated():
		duplicated_out += 1
		_enqueue_outbound(_pending_out, due, false, op, payload)
	return true


## Counts one bare wire-code send (join ack, leave, room request) the same way;
## note a wire code may share an opcode slot with a canonical op, as the
## protocol's two spaces overlap by design.
func send_wire(code: int, body: PackedByteArray) -> bool:
	if profiler != null and profiler.enabled:
		profiler.record_send(CYGOpcodes.from_wire(code), body.size())
	if not degrade_enabled or not inner.is_connected:
		return inner.send_wire(code, body)
	if _roll_dropped():
		dropped_out += 1
		return true
	var now := _clock_now()
	var due := now + _delay_seconds()
	_enqueue_outbound(_pending_out, due, true, code, body)
	if _roll_duplicated():
		duplicated_out += 1
		_enqueue_outbound(_pending_out, due, true, code, body)
	return true


func connect_socket(url: String, auth_token: String = "") -> void:
	_pending_in.clear()
	_pending_out.clear()
	inner.connect_socket(url, auth_token)


func connect_to_room(room_name: String, player_name: String = "") -> void:
	inner.connect_to_room(room_name, player_name)


func reconnect_to_room(room_name: String, reconnection_token: String) -> void:
	inner.reconnect_to_room(room_name, reconnection_token)


## Answers the server-initiated join through this wrapper so the byte counts
## stay complete (`inner`'s own `ack_join` would bypass the tap).
func ack_join() -> void:
	send_wire(CYGOpcodes.WireJoinRoom, PackedByteArray())


func disconnect_from_room() -> void:
	if inner.is_connected:
		send_wire(CYGOpcodes.WireLeaveRoom, PackedByteArray())
	close()


func close() -> void:
	inner.close()
	is_connected = inner.is_connected
	last_close_code = inner.last_close_code
	_pending_in.clear()
	_pending_out.clear()


## Forwards the drop to the wrapped socket. The base implementation would close this
## wrapper's own unused `WebSocketPeer`, reporting a connection the room never had.
func simulate_drop(reason: String = "simulated-drop", code: int = CYGOpcodes.CloseMayTryReconnect) -> void:
	inner.simulate_drop(reason, code)


func session_id() -> String:
	return inner.session_id()


func reconnection_token() -> String:
	return inner.reconnection_token()


func session_token() -> String:
	return inner.session_token()


## ------------------------------------------------------------------ internals

## Test seam: the degradation clock. A scripted subclass overrides this to run
## latency without sleeping.
func _clock_now() -> float:
	return Time.get_ticks_msec() / 1000.0


func _sync_callbacks_to_inner() -> void:
	if host != null and inner.host != host:
		inner.host = host
	if settings != null and inner.settings != settings:
		inner.settings = settings
	# The ping probe is emitted from `inner.poll()` itself, so the RTT callback
	# the autoload installed is chained through here: counting the outbound ping
	# (always 4 payload bytes, one big-endian u32) and then timestamping.
	if inner.on_ping_sent != _ping_chain:
		inner.on_ping_sent = _ping_chain


func _on_inner_ping_sent() -> void:
	if profiler != null and profiler.enabled:
		profiler.record_send(CYGOpcodes.Ping, 4)
	if on_ping_sent.is_valid():
		on_ping_sent.call()


func _move_connections(network: Object) -> void:
	for plan in _TAP_PLAN:
		var signal_name: String = plan[0]
		var tap := Signal(inner, signal_name)
		for connection in tap.get_connections():
			var handler: Callable = connection["callable"]
			if handler.get_object() != network:
				continue
			tap.disconnect(handler)
			Signal(self, signal_name).connect(handler)
			_moved.append({"signal": signal_name, "callable": handler})


func _tap_inner_signals() -> void:
	for plan in _TAP_PLAN:
		var signal_name: String = plan[0]
		var tap := Signal(inner, signal_name)
		var handler := Callable(self, plan[1])
		if not tap.is_connected(handler):
			tap.connect(handler)
		_tapped.append({"signal": tap, "handler": handler})


func _on_inner_frame(frame: PackedByteArray) -> void:
	if profiler != null and profiler.enabled:
		_record_inbound_frame(frame)
	if not degrade_enabled:
		frame_received.emit(frame)
		return
	if _roll_dropped():
		dropped_in += 1
		return
	var due := _clock_now() + _delay_seconds()
	_enqueue_inbound(due, frame)
	if _roll_duplicated():
		duplicated_in += 1
		_enqueue_inbound(due, frame)


func _on_inner_socket_opened() -> void:
	socket_opened.emit()


func _on_inner_socket_closed(reason: String, code: int) -> void:
	_pending_in.clear()
	_pending_out.clear()
	socket_closed.emit(reason, code)


func _on_inner_server_error(detail: String) -> void:
	server_error.emit(detail)


func _on_inner_seat_ready(seat: CYGTypes.Seat) -> void:
	seat_ready.emit(seat)


func _on_inner_seat_failed(error: String) -> void:
	seat_failed.emit(error)


func _record_inbound_frame(frame: PackedByteArray) -> void:
	if frame.size() < CYGOpcodes.HEADER_SIZE:
		return
	var op := frame[2]
	var payload_bytes := frame.size() - CYGOpcodes.HEADER_SIZE
	profiler.record_receive(op, payload_bytes)
	# A body-less StateDelta has no view id to decode; reading it would raise a
	# reader underflow on an empty-but-framed packet.
	if op == CYGOpcodes.StateDelta and payload_bytes > 0:
		_scratch_reader.attach(frame, CYGOpcodes.HEADER_SIZE)
		var view_id := _scratch_reader.read_int32()
		var body_bytes := maxi(frame.size() - CYGOpcodes.HEADER_SIZE - _scratch_reader.bytes_read, 0)
		profiler.record_view_delta_in(view_id, body_bytes)


func _attribute_outbound_view_delta(payload: PackedByteArray) -> void:
	if payload.is_empty():
		return
	_scratch_reader.attach(payload, 0)
	var view_id := _scratch_reader.read_int32()
	var body_bytes := maxi(payload.size() - _scratch_reader.bytes_read, 0)
	profiler.record_view_delta_out(view_id, body_bytes)


func _roll_dropped() -> bool:
	return drop_percent > 0.0 and _rng.randf() * 100.0 < drop_percent


func _roll_duplicated() -> bool:
	return duplicate_percent > 0.0 and _rng.randf() * 100.0 < duplicate_percent


func _delay_seconds() -> float:
	var offset := 0.0
	if jitter_ms > 0.0:
		offset = (_rng.randf() * 2.0 - 1.0) * jitter_ms
	return maxf(latency_ms + offset, 0.0) / 1000.0


func _enqueue_inbound(due: float, frame: PackedByteArray) -> void:
	if _pending_in.size() >= MAX_DEGRADE_QUEUE:
		var oldest: Dictionary = _pending_in.pop_front()
		frame_received.emit(oldest["frame"])
	_pending_in.append({"due": due, "frame": frame})


func _enqueue_outbound(queue: Array, due: float, is_wire: bool, code: int, body: PackedByteArray) -> void:
	if queue.size() >= MAX_DEGRADE_QUEUE:
		var oldest: Dictionary = queue.pop_front()
		_deliver_outbound(oldest)
	queue.append({"due": due, "wire": is_wire, "code": code, "body": body})


func _flush_due(now: float) -> void:
	while not _pending_in.is_empty():
		var head_in: Dictionary = _pending_in[0]
		if float(head_in["due"]) > now:
			break
		_pending_in.pop_front()
		frame_received.emit(head_in["frame"])
	while not _pending_out.is_empty():
		var head_out: Dictionary = _pending_out[0]
		if float(head_out["due"]) > now:
			break
		_pending_out.pop_front()
		_deliver_outbound(head_out)


func _deliver_outbound(entry: Dictionary) -> void:
	if entry["wire"]:
		inner.send_wire(entry["code"], entry["body"])
	else:
		inner.send_frame(entry["code"], entry["body"])
