class_name CYGNetworkProfiler
extends RefCounted
## Rolling one-second traffic ledger for developer observability.
##
## Records what the transport saw (bytes and packets per direction and per
## canonical opcode), what the rollback layer saw (rewind hits and mispredicted
## ticks) and what the room clock did (local tick cadence against the nominal
## `CYGNetwork.tick_rate`). Windows are committed into a fixed ring of the last
## `RING_CAPACITY` seconds; every accessor reads finished measurements, never a
## number that changes underneath it.
##
## The record path is allocation-free by construction: ring storage is a handful
## of pre-sized `Packed*Array`s indexed slot-first, per-opcode totals live in
## two fixed arrays, and the only dictionaries are the per-view delta tables,
## whose steady-state writes update existing int keys. Bucket rotation happens
## in `pump()`, once per frame from whoever owns the clock, never per packet.
## When `enabled` is false every entry point returns after one branch and
## touches nothing, so the SDK's default startup stays inert.
##
## `snapshot()` returns a plain Dictionary with exactly these keys:
##
##  * `enabled`, `time`, `window_seconds`, `ring_capacity`, `sample_count`,
##    `head` — clock state; `head` indexes the oldest valid ring slot, and
##    `sample_count` grows to `ring_capacity` then holds.
##  * `samples` — `Array[Dictionary]`, oldest window first, at most
##    `ring_capacity` entries, each with `started_at`, `bytes_sent`,
##    `bytes_received`, `packets_sent`, `packets_received`, `tick_count`,
##    `tick_sum_ms`, `max_tick_ms`, `rtt_min_ms`, `rtt_avg_ms`, `rtt_max_ms`,
##    `rtt_jitter_ms`, `mispredictions`, `rewinds`.
##  * `live` — the in-progress (uncommitted) window: `bytes_sent`,
##    `bytes_received`, `packets_sent`, `packets_received`.
##  * `totals` — lifetime sums since `reset()`: `bytes_sent`,
##    `bytes_received`, `packets_sent`, `packets_received`, `mispredictions`,
##    `rewinds`.
##  * `rtt` — ring-wide RTT summary: `current_ms`, `min_ms`, `avg_ms`,
##    `max_ms`, `jitter_ms` (average absolute change between consecutive
##    samples; 0 for fewer than two).
##  * `tick` — `hz` (committed windows averaged), `nominal_hz`, `ratio`,
##    `max_tick_ms`.
##  * `rollback` — last committed window: `mispredictions`, `rewinds`.
##  * `opcode_bytes` — last committed window, `{"sent": {op: bytes},
##    "received": {op: bytes}}`, only non-zero opcodes listed.
##  * `view_delta_bytes` — the last committed window (or the open one before
##    the first commit), `{"out": {view_id: bytes}, "in": {view_id: bytes}}`
##    with view-delta payload bytes (the varint view-id prefix excluded); an
##    internal overflow bucket keyed `-1` collects views beyond
##    `MAX_TRACKED_VIEWS`.
##
## Usage:
##
##     var profiler := CYGNetworkProfiler.new()
##     profiler.enabled = true
##     CYGNetworkProfiler.active = profiler
##     profiler.bind_network(CYGNetwork)   # ticks, rewind hits, RTT sampling
##     # ... per frame, from the owner of the clock:
##     profiler.pump_realtime()
##     # ... per counted packet:
##     profiler.record_send(CYGOpcodes.CustomRpc, payload.size())
##     var snap: Dictionary = profiler.snapshot()
##     print(snap["live"]["bytes_sent"], " bytes in the open window")
##
## Design derives from Netfox's MIT-licensed profiler and Mirror's interest/transport metrics (attribution in LICENSE-NETFOX.md). Clean-room GDScript; no upstream source text is copied.

signal sample_taken

## One-second windows retained, oldest dropped first.
const RING_CAPACITY := 60
## Seconds per committed window.
const WINDOW_SECONDS := 1.0
## Canonical opcode slots, the highest being `CYGOpcodes.DespawnObject` (21).
const OPCODE_SLOTS := 22
## Per-view delta tables stop growing here; late views fold into key `-1` so
## window byte totals stay exact regardless.
const MAX_TRACKED_VIEWS := 64
## Windows force-committed after a long stall before the clock re-anchors.
const MAX_COMMIT_CATCH_UP := 8

## The profiler a HUD should read; set by `CYGProfilingTransport.attach()`.
static var active: CYGNetworkProfiler = null

## Master switch. False by default: nothing records, nothing rotates.
var enabled: bool = false:
	set(value):
		if value == enabled:
			return
		enabled = value
		if value:
			# Re-anchor the first window on the next pump instead of committing
			# the gap that the disabled period left behind.
			_window_started = false

var nominal_tick_rate: int = 0

var _ring_bytes_sent := PackedInt64Array()
var _ring_bytes_received := PackedInt64Array()
var _ring_packets_sent := PackedInt32Array()
var _ring_packets_received := PackedInt32Array()
var _ring_ticks := PackedInt32Array()
var _ring_tick_sum_ms := PackedFloat64Array()
var _ring_max_tick_ms := PackedFloat64Array()
var _ring_started_at := PackedFloat64Array()
var _ring_rtt_sum_ms := PackedFloat64Array()
var _ring_rtt_count := PackedInt32Array()
var _ring_rtt_min_ms := PackedFloat64Array()
var _ring_rtt_max_ms := PackedFloat64Array()
var _ring_rtt_jitter_sum_ms := PackedFloat64Array()
var _ring_rtt_jitter_count := PackedInt32Array()
var _ring_mispredictions := PackedInt32Array()
var _ring_rewinds := PackedInt32Array()
var _ring_opcode_sent := PackedInt64Array()
var _ring_opcode_received := PackedInt64Array()

var _live_opcode_sent := PackedInt64Array()
var _live_opcode_received := PackedInt64Array()
var _views_live_out := {}
var _views_live_in := {}
var _views_committed_out := {}
var _views_committed_in := {}

var _window_bytes_sent := 0
var _window_bytes_received := 0
var _window_packets_sent := 0
var _window_packets_received := 0
var _window_ticks := 0
var _window_tick_sum_ms := 0.0
var _window_max_tick_ms := 0.0
var _window_rtt_sum_ms := 0.0
var _window_rtt_count := 0
var _window_rtt_min_ms := INF
var _window_rtt_max_ms := 0.0
var _window_rtt_jitter_sum_ms := 0.0
var _window_rtt_jitter_count := 0
var _window_rtt_last := INF
var _window_mispredictions := 0
var _window_rewinds := 0

var _window_opened_at := 0.0
var _window_started := false
var _write_index := 0
var _fill_count := 0

var _total_bytes_sent := 0
var _total_bytes_received := 0
var _total_packets_sent := 0
var _total_packets_received := 0
var _total_mispredictions := 0
var _total_rewinds := 0

var _last_rtt_ms := 0.0
var _last_latency_sample := -1.0
var _last_tick_at := -1.0
var _now := 0.0
var _network: Object = null
var _bound_connections: Array = []
var _watched_synchronizers: Array = []

# Reusable reader so attributing a view delta allocates nothing per packet.
var _scratch_reader := CYGBitReader.new()


func _init() -> void:
	_ring_bytes_sent.resize(RING_CAPACITY)
	_ring_bytes_received.resize(RING_CAPACITY)
	_ring_packets_sent.resize(RING_CAPACITY)
	_ring_packets_received.resize(RING_CAPACITY)
	_ring_ticks.resize(RING_CAPACITY)
	_ring_tick_sum_ms.resize(RING_CAPACITY)
	_ring_max_tick_ms.resize(RING_CAPACITY)
	_ring_started_at.resize(RING_CAPACITY)
	_ring_rtt_sum_ms.resize(RING_CAPACITY)
	_ring_rtt_count.resize(RING_CAPACITY)
	_ring_rtt_min_ms.resize(RING_CAPACITY)
	_ring_rtt_max_ms.resize(RING_CAPACITY)
	_ring_rtt_jitter_sum_ms.resize(RING_CAPACITY)
	_ring_rtt_jitter_count.resize(RING_CAPACITY)
	_ring_mispredictions.resize(RING_CAPACITY)
	_ring_rewinds.resize(RING_CAPACITY)
	_ring_opcode_sent.resize(RING_CAPACITY * OPCODE_SLOTS)
	_ring_opcode_received.resize(RING_CAPACITY * OPCODE_SLOTS)
	_live_opcode_sent.resize(OPCODE_SLOTS)
	_live_opcode_received.resize(OPCODE_SLOTS)


## ------------------------------------------------------------------ recording

## Counts one outbound packet. `payload_bytes` excludes the 3-byte canonical
## header; opcodes outside 0..21 still count toward the totals but not a slot.
func record_send(op: int, payload_bytes: int) -> void:
	if not enabled:
		return
	_advance_window()
	var size := maxi(payload_bytes, 0)
	_window_bytes_sent += size
	_window_packets_sent += 1
	_total_bytes_sent += size
	_total_packets_sent += 1
	if op >= 0 and op < OPCODE_SLOTS:
		_live_opcode_sent[op] += size


## Counts one inbound packet; same opcode and header rules as `record_send()`.
func record_receive(op: int, payload_bytes: int) -> void:
	if not enabled:
		return
	_advance_window()
	var size := maxi(payload_bytes, 0)
	_window_bytes_received += size
	_window_packets_received += 1
	_total_bytes_received += size
	_total_packets_received += 1
	if op >= 0 and op < OPCODE_SLOTS:
		_live_opcode_received[op] += size


## Attributes view-delta payload bytes (the `[zigzag varint viewId]` prefix
## already stripped) leaving this peer for `view_id` in the open window.
func record_view_delta_out(view_id: int, body_bytes: int) -> void:
	if not enabled:
		return
	_accumulate_view(_views_live_out, view_id, body_bytes)


## Attributes view-delta payload bytes received for `view_id`.
func record_view_delta_in(view_id: int, body_bytes: int) -> void:
	if not enabled:
		return
	_accumulate_view(_views_live_in, view_id, body_bytes)


## Decodes the `[zigzag varint viewId][body]` framing `CYGNetwork
## .send_view_delta()` and `_handle_view_delta()` use, then attributes `body`
## size to the view. Called by `CYGProfilingTransport` per StateDelta packet.
func record_view_delta_payload(view_id: int, payload: PackedByteArray) -> void:
	if not enabled:
		return
	_scratch_reader.attach(payload)
	_scratch_reader.read_int32()
	var body_bytes := maxi(payload.size() - _scratch_reader.bytes_read, 0)
	_accumulate_view(_views_live_out, view_id, body_bytes)


## Adds one observed network-tick interval in milliseconds to the window.
func record_tick(delta_ms: float) -> void:
	if not enabled:
		return
	_advance_window()
	_window_ticks += 1
	_window_tick_sum_ms += delta_ms
	if delta_ms > _window_max_tick_ms:
		_window_max_tick_ms = delta_ms


## Adds one round-trip measurement in milliseconds. Jitter is the mean
## absolute change between consecutive samples inside a window.
func record_latency(latency_ms: float) -> void:
	if not enabled:
		return
	_advance_window()
	var sample := maxf(latency_ms, 0.0)
	_last_rtt_ms = sample
	_window_rtt_sum_ms += sample
	_window_rtt_count += 1
	_window_rtt_min_ms = minf(_window_rtt_min_ms, sample)
	_window_rtt_max_ms = maxf(_window_rtt_max_ms, sample)
	if _window_rtt_last < INF:
		_window_rtt_jitter_sum_ms += absf(sample - _window_rtt_last)
		_window_rtt_jitter_count += 1
	_window_rtt_last = sample


## Counts `count` ticks that had to be replayed against the authority's state.
func record_mispredictions(count: int = 1) -> void:
	if not enabled:
		return
	_advance_window()
	_window_mispredictions += count
	_total_mispredictions += count


## Counts one rollback reconciliation (a rewind actually replayed ticks).
func record_rewind() -> void:
	if not enabled:
		return
	_advance_window()
	_window_rewinds += 1
	_total_rewinds += 1


## --------------------------------------------------------------------- clock

## Rolls the wall clock: commits every elapsed one-second window into the ring.
## `now_seconds` is injectable (parity with the Unreal SDK's `Pump(NowSeconds)`)
## so tests and the editor dock drive the cadence without sleeping. Idempotent
## between boundaries: one float comparison per call on the hot frame.
func pump(now_seconds: float) -> void:
	if not enabled:
		return
	_now = now_seconds
	_sample_network_state()
	_advance_window_at(now_seconds)


## `pump()` against the engine millisecond clock.
func pump_realtime() -> void:
	pump(Time.get_ticks_msec() / 1000.0)


## The last clock handed to `pump()`; tick intervals are measured against it.
func now_seconds() -> float:
	return _now


## Force-commits the open window (parity with the Unity SDK's `CommitWindow()`),
## so a final partial second can be read. No-op before anything was recorded.
func commit_now() -> void:
	if not enabled:
		return
	_commit(_window_opened_at if _window_started else _now)
	_window_opened_at = _now
	_window_started = true


## ------------------------------------------------------------------- binding

## Connects to the seams the room facade exposes: `network_tick` for cadence,
## `rewind_hit_received` for rollback events, and `latency_ms` sampled once per
## `pump()` for RTT. `network` defaults to the autoload. A node is only required
## to *have* those members, so test doubles and the real autoload bind alike.
func bind_network(network: Object = null) -> bool:
	if network == null:
		var loop := Engine.get_main_loop() as SceneTree
		network = loop.root.get_node_or_null("CYGNetwork") if loop != null else null
	if network == null or _network == network:
		return network != null
	_network = network
	var tick_rate: Variant = network.get("tick_rate")
	if tick_rate != null:
		nominal_tick_rate = int(tick_rate)
	_try_bind(network, "network_tick", _on_network_tick)
	_try_bind(network, "rewind_hit_received", _on_rewind_hit)
	return true


func unbind_network() -> void:
	for entry in _bound_connections:
		var signal_obj: Signal = entry["signal"]
		var handler: Callable = entry["callable"]
		if signal_obj.get_object() != null and signal_obj.is_connected(handler):
			signal_obj.disconnect(handler)
	_bound_connections.clear()
	_network = null
	_last_tick_at = -1.0
	_last_latency_sample = -1.0


## Connects to one `CYGRollbackSynchronizer` ledger's `reconciled` signal.
##
## EXTENSION POINT: the SDK gives each predicting component its own ledger
## (`CYGPredictiveSync` and `CYGCharacterSync2D/3D` each construct one), so
## there is no global list to enumerate from here. Mispredict and rewind counts
## arrive from inbound RewindHit packets and from these explicit watches until
## the core exposes one shared signal (see the profiler integration notes).
func watch_synchronizer(synchronizer: Object) -> void:
	if synchronizer == null or synchronizer in _watched_synchronizers:
		return
	if not synchronizer.has_signal("reconciled"):
		push_warning("CYGNetworkProfiler.watch_synchronizer: object has no `reconciled` signal.")
		return
	synchronizer.reconciled.connect(_on_reconciled)
	_watched_synchronizers.append(synchronizer)


func unwatch_synchronizer(synchronizer: Object) -> void:
	var index := _watched_synchronizers.find(synchronizer)
	if index < 0:
		return
	if is_instance_valid(synchronizer) and synchronizer.reconciled.is_connected(_on_reconciled):
		synchronizer.reconciled.disconnect(_on_reconciled)
	_watched_synchronizers.remove_at(index)


## ----------------------------------------------------------------- reporting

## Builds the plain-Dictionary view documented at the top of this file. The
## `samples` array is copied out of the ring (bounded, oldest first), so callers
## may keep it; the two breakdown dictionaries are copied too. Call at HUD
## rates (a few Hz), not per packet.
func snapshot() -> Dictionary:
	var head := _write_index if _fill_count == RING_CAPACITY else 0
	var samples: Array = []
	samples.resize(_fill_count)
	var tick_total := 0
	var rtt_sum := 0.0
	var rtt_count := 0
	var rtt_min := INF
	var rtt_max := 0.0
	var jitter_sum := 0.0
	var jitter_count := 0
	var mispredict_total := 0
	var rewind_total := 0
	var max_tick_ms := 0.0
	for ordinal in range(_fill_count):
		var slot := (head + ordinal) % RING_CAPACITY
		var rtt_slot_count := _ring_rtt_count[slot]
		var window := {
			"started_at": _ring_started_at[slot],
			"bytes_sent": _ring_bytes_sent[slot],
			"bytes_received": _ring_bytes_received[slot],
			"packets_sent": _ring_packets_sent[slot],
			"packets_received": _ring_packets_received[slot],
			"tick_count": _ring_ticks[slot],
			"tick_sum_ms": _ring_tick_sum_ms[slot],
			"max_tick_ms": _ring_max_tick_ms[slot],
			"rtt_min_ms": (_ring_rtt_min_ms[slot] if rtt_slot_count > 0 else 0.0),
			"rtt_avg_ms": (_ring_rtt_sum_ms[slot] / rtt_slot_count if rtt_slot_count > 0 else 0.0),
			"rtt_max_ms": (_ring_rtt_max_ms[slot] if rtt_slot_count > 0 else 0.0),
			"rtt_jitter_ms": (_ring_rtt_jitter_sum_ms[slot] / _ring_rtt_jitter_count[slot]
				if _ring_rtt_jitter_count[slot] > 0 else 0.0),
			"mispredictions": _ring_mispredictions[slot],
			"rewinds": _ring_rewinds[slot],
		}
		samples[ordinal] = window
		tick_total += _ring_ticks[slot]
		mispredict_total += _ring_mispredictions[slot]
		rewind_total += _ring_rewinds[slot]
		max_tick_ms = maxf(max_tick_ms, _ring_max_tick_ms[slot])
		if rtt_slot_count > 0:
			rtt_sum += _ring_rtt_sum_ms[slot]
			rtt_count += rtt_slot_count
			rtt_min = minf(rtt_min, _ring_rtt_min_ms[slot])
			rtt_max = maxf(rtt_max, _ring_rtt_max_ms[slot])
			jitter_sum += _ring_rtt_jitter_sum_ms[slot]
			jitter_count += _ring_rtt_jitter_count[slot]

	var window_span := float(_fill_count) * WINDOW_SECONDS
	var hz := float(tick_total) / window_span if window_span > 0.0 else 0.0
	var ratio := hz / float(nominal_tick_rate) if nominal_tick_rate > 0 else 0.0

	var opcode_sent := {}
	var opcode_received := {}
	for op in range(OPCODE_SLOTS):
		var slot_base := _most_recent_slot() * OPCODE_SLOTS + op
		var sent_value: int = _ring_opcode_sent[slot_base] if _fill_count > 0 else _live_opcode_sent[op]
		var received_value: int = _ring_opcode_received[slot_base] if _fill_count > 0 else _live_opcode_received[op]
		if sent_value != 0:
			opcode_sent[op] = sent_value
		if received_value != 0:
			opcode_received[op] = received_value

	return {
		"enabled": enabled,
		"time": _now,
		"window_seconds": WINDOW_SECONDS,
		"ring_capacity": RING_CAPACITY,
		"sample_count": _fill_count,
		"head": head,
		"samples": samples,
		"live": {
			"bytes_sent": _window_bytes_sent,
			"bytes_received": _window_bytes_received,
			"packets_sent": _window_packets_sent,
			"packets_received": _window_packets_received,
		},
		"totals": {
			"bytes_sent": _total_bytes_sent,
			"bytes_received": _total_bytes_received,
			"packets_sent": _total_packets_sent,
			"packets_received": _total_packets_received,
			"mispredictions": _total_mispredictions,
			"rewinds": _total_rewinds,
		},
		"rtt": {
			"current_ms": _last_rtt_ms,
			"min_ms": (rtt_min if rtt_min < INF else 0.0),
			"avg_ms": (rtt_sum / rtt_count if rtt_count > 0 else 0.0),
			"max_ms": (rtt_max if rtt_count > 0 else 0.0),
			"jitter_ms": (jitter_sum / jitter_count if jitter_count > 0 else 0.0),
		},
		"tick": {
			"hz": hz,
			"nominal_hz": float(nominal_tick_rate),
			"ratio": ratio,
			"max_tick_ms": max_tick_ms,
		},
		"rollback": {
			"mispredictions": (_ring_mispredictions[_most_recent_slot()] if _fill_count > 0 else _window_mispredictions),
			"rewinds": (_ring_rewinds[_most_recent_slot()] if _fill_count > 0 else _window_rewinds),
		},
		"opcode_bytes": {"sent": opcode_sent, "received": opcode_received},
		# Before the first commit there is no finished window to report, so the
		# live tables stand in (same fallback rule as the opcode totals above).
		"view_delta_bytes": {
			"out": (_views_live_out if _fill_count == 0 else _views_committed_out).duplicate(),
			"in": (_views_live_in if _fill_count == 0 else _views_committed_in).duplicate(),
		},
	}


## Clears ring, windows and breakdowns; leaves `enabled` and bindings alone.
func reset() -> void:
	_ring_bytes_sent.fill(0)
	_ring_bytes_received.fill(0)
	_ring_packets_sent.fill(0)
	_ring_packets_received.fill(0)
	_ring_ticks.fill(0)
	_ring_tick_sum_ms.fill(0.0)
	_ring_max_tick_ms.fill(0.0)
	_ring_started_at.fill(0.0)
	_ring_rtt_sum_ms.fill(0.0)
	_ring_rtt_count.fill(0)
	_ring_rtt_min_ms.fill(0.0)
	_ring_rtt_max_ms.fill(0.0)
	_ring_rtt_jitter_sum_ms.fill(0.0)
	_ring_rtt_jitter_count.fill(0)
	_ring_mispredictions.fill(0)
	_ring_rewinds.fill(0)
	_ring_opcode_sent.fill(0)
	_ring_opcode_received.fill(0)
	_reset_window()
	_window_opened_at = 0.0
	_views_committed_out.clear()
	_views_committed_in.clear()
	_total_bytes_sent = 0
	_total_bytes_received = 0
	_total_packets_sent = 0
	_total_packets_received = 0
	_total_mispredictions = 0
	_total_rewinds = 0
	_last_rtt_ms = 0.0
	_last_latency_sample = -1.0
	_last_tick_at = -1.0
	_window_started = false


## ------------------------------------------------------------------ internals

func _try_bind(target: Object, signal_name: String, handler: Callable) -> void:
	if not target.has_signal(signal_name):
		return
	var signal_obj := Signal(target, signal_name)
	if signal_obj.is_connected(handler):
		return
	signal_obj.connect(handler)
	_bound_connections.append({"signal": signal_obj, "callable": handler})


func _on_network_tick(_tick: int) -> void:
	# Interval measured on the pump clock so an injected clock controls it.
	if _now <= 0.0:
		return
	if _last_tick_at >= 0.0:
		record_tick((_now - _last_tick_at) * 1000.0)
	_last_tick_at = _now


func _on_rewind_hit(_payload: PackedByteArray) -> void:
	record_rewind()


func _on_reconciled(_object_id: int, _from_tick: int, _to_tick: int, mispredicted: int) -> void:
	record_rewind()
	if mispredicted > 0:
		record_mispredictions(mispredicted)


func _sample_network_state() -> void:
	if _network == null or not is_instance_valid(_network):
		return
	var tick_rate: Variant = _network.get("tick_rate")
	if tick_rate != null:
		nominal_tick_rate = int(tick_rate)
	# `latency_ms` only changes when a pong answers a ping (every few seconds),
	# so an inequality against the last observed value means a fresh RTT sample.
	var latency: Variant = _network.get("latency_ms")
	if latency != null and not (latency is float and is_nan(latency)):
		var ms := float(latency)
		if ms > 0.0 and ms != _last_latency_sample:
			_last_latency_sample = ms
			record_latency(ms)


func _advance_window() -> void:
	# Recordings advance on the clock the last `pump()` handed in, never on a
	# private wall-clock read: mixing the two would commit a half-filled window
	# mid-burst whenever the engine booted slowly. Without any pump the open
	# window simply keeps accumulating into fixed storage.
	_advance_window_at(_now)


func _advance_window_at(now_seconds: float) -> void:
	if not _window_started:
		_window_opened_at = now_seconds
		_window_started = true
		return
	var committed := 0
	while now_seconds - _window_opened_at >= WINDOW_SECONDS and committed < MAX_COMMIT_CATCH_UP:
		_commit(_window_opened_at)
		_window_opened_at += WINDOW_SECONDS
		committed += 1
	# A stall longer than the catch-up collapses into one fresh window opened
	# now, rather than a burst of fabricated empty ones.
	if now_seconds - _window_opened_at >= WINDOW_SECONDS:
		_window_opened_at = now_seconds


func _commit(started_at: float) -> void:
	_ring_bytes_sent[_write_index] = _window_bytes_sent
	_ring_bytes_received[_write_index] = _window_bytes_received
	_ring_packets_sent[_write_index] = _window_packets_sent
	_ring_packets_received[_write_index] = _window_packets_received
	_ring_ticks[_write_index] = _window_ticks
	_ring_tick_sum_ms[_write_index] = _window_tick_sum_ms
	_ring_max_tick_ms[_write_index] = _window_max_tick_ms
	_ring_started_at[_write_index] = started_at
	_ring_rtt_sum_ms[_write_index] = _window_rtt_sum_ms
	_ring_rtt_count[_write_index] = _window_rtt_count
	_ring_rtt_min_ms[_write_index] = (_window_rtt_min_ms if _window_rtt_count > 0 else 0.0)
	_ring_rtt_max_ms[_write_index] = (_window_rtt_max_ms if _window_rtt_count > 0 else 0.0)
	_ring_rtt_jitter_sum_ms[_write_index] = _window_rtt_jitter_sum_ms
	_ring_rtt_jitter_count[_write_index] = _window_rtt_jitter_count
	_ring_mispredictions[_write_index] = _window_mispredictions
	_ring_rewinds[_write_index] = _window_rewinds
	var base := _write_index * OPCODE_SLOTS
	for op in range(OPCODE_SLOTS):
		_ring_opcode_sent[base + op] = _live_opcode_sent[op]
		_ring_opcode_received[base + op] = _live_opcode_received[op]

	# Recycle the two view tables instead of allocating fresh ones per window;
	# the committed pair is what `snapshot()` reports until the next commit.
	var swap_out := _views_live_out
	_views_live_out = _views_committed_out
	_views_committed_out = swap_out
	_views_live_out.clear()
	var swap_in := _views_live_in
	_views_live_in = _views_committed_in
	_views_committed_in = swap_in
	_views_live_in.clear()

	_write_index = _write_index + 1 if _write_index + 1 < RING_CAPACITY else 0
	if _fill_count < RING_CAPACITY:
		_fill_count += 1
	# `_reset_window` zeroes accumulators only; the caller owns the window
	# position so the clock advances exactly one WINDOW_SECONDS per commit.
	_reset_window()
	sample_taken.emit()


func _reset_window() -> void:
	_window_bytes_sent = 0
	_window_bytes_received = 0
	_window_packets_sent = 0
	_window_packets_received = 0
	_window_ticks = 0
	_window_tick_sum_ms = 0.0
	_window_max_tick_ms = 0.0
	_window_rtt_sum_ms = 0.0
	_window_rtt_count = 0
	_window_rtt_min_ms = INF
	_window_rtt_max_ms = 0.0
	_window_rtt_jitter_sum_ms = 0.0
	_window_rtt_jitter_count = 0
	_window_rtt_last = INF
	_window_mispredictions = 0
	_window_rewinds = 0
	_live_opcode_sent.fill(0)
	_live_opcode_received.fill(0)


func _accumulate_view(table: Dictionary, view_id: int, body_bytes: int) -> void:
	var amount := maxi(body_bytes, 0)
	if amount == 0:
		return
	_advance_window()
	if table.has(view_id):
		table[view_id] = int(table[view_id]) + amount
	elif table.size() < MAX_TRACKED_VIEWS or view_id == -1:
		table[view_id] = amount
	else:
		table[-1] = int(table.get(-1, 0)) + amount


func _most_recent_slot() -> int:
	return _write_index - 1 if _write_index > 0 else RING_CAPACITY - 1
