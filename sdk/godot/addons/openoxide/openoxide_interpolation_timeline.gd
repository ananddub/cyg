class_name OpenOxideInterpolationTimeline
extends RefCounted
## Tick-authoritative remote-entity smoother (Lightyear-style), ported from
## `OpenOxideInterpolationTimeline` in the JS SDK.
##
## Snapshots are keyed by server tick. A render clock tracks server ticks,
## renders `interp_ticks` in the past, grows a jitter margin when arrivals are
## bursty, accelerates when it falls too far behind, and snaps once instead of
## stalling across a long packet-loss gap.
##
## Entities are expected as `id -> Array[float]`; pass `lerp_fn` for a custom
## shape, or `field_lerp` / `discrete` for per-field control.

var interp_ticks := 2
var speedup_factor := 1.05
var snap_ticks := 10
var max_ticks_per_step := 4
var lerp_fn: Callable
var field_lerp: Dictionary  # column index -> Callable(a, b, alpha)
var discrete: Array = []    # column indices copied from the newer snapshot

var snaps: Array = []
var max_snaps := 64
var tick_duration_ms := 15.625
var jitter_ms := 0.0
var render_tick := -INF

var _nominal_tick_ms := 15.625
var _last_arrival_ms := -1.0
var _last_delta_ms := 15.625
var _last_sample_ms := -1.0
var _margin := -1
var _target_ticks := 2
var _margin_changed_ms := 0.0
var _speedup_ticks := 0
var _snap_count := 0


func _init(options: Dictionary = {}) -> void:
	interp_ticks = int(options.get("interpTicks", 2))
	speedup_factor = float(options.get("speedupFactor", 1.05))
	snap_ticks = int(options.get("snapTicks", 10))
	max_ticks_per_step = int(options.get("maxTicksPerStep", 4))
	lerp_fn = options.get("lerpFn", Callable())
	field_lerp = options.get("fieldLerp", {})
	discrete = options.get("discrete", [])
	max_snaps = int(options.get("maxSnaps", 64))
	tick_duration_ms = float(options.get("tickDurationMs", 15.625))
	_nominal_tick_ms = tick_duration_ms
	_last_delta_ms = tick_duration_ms
	_target_ticks = interp_ticks


## Record an authoritative snapshot. A same-tick retransmit replaces the older
## copy; a tick behind everything buffered is dropped.
func push_snapshot(tick: int, entities: Variant, arrival_ms: float = -1.0) -> void:
	if snaps.is_empty() == false and tick < int(snaps.back()["tick"]):
		return
	var now := arrival_ms
	if now < 0.0:
		now = float(Time.get_ticks_msec())
	if not snaps.is_empty() and tick == int(snaps.back()["tick"]):
		snaps[snaps.size() - 1] = {"tick": tick, "timeMs": now, "entities": entities}
		return

	if _last_arrival_ms >= 0.0 and not snaps.is_empty():
		var gap := tick - int(snaps.back()["tick"])
		var delta_ms := now - _last_arrival_ms
		if delta_ms >= 0.0 and gap <= 64:
			var per_tick_ms := delta_ms / float(gap)
			jitter_ms = jitter_ms * 0.9 + absf(delta_ms - gap * _last_delta_ms) * 0.1
			var weight := minf(0.15, 1.0 / float(snaps.size() + 2))
			_last_delta_ms = _last_delta_ms * (1.0 - weight) + per_tick_ms * weight
			tick_duration_ms = maxf(_nominal_tick_ms * 0.8,
				minf(_nominal_tick_ms * 1.25, _last_delta_ms))
	_last_arrival_ms = now

	snaps.append({"tick": tick, "timeMs": now, "entities": entities})
	if snaps.size() == 1:
		render_tick = float(tick)
		_last_sample_ms = now
		_target_ticks = interp_ticks + mini(6, int(ceil(jitter_ms * 4.0 / tick_duration_ms)))

	while snaps.size() > max_snaps:
		snaps.pop_front()


## Advance the clock (once per frame) and return the blended snapshot, or null.
func sample(now_ms: float = -1.0) -> Variant:
	if snaps.is_empty():
		return null
	var now := now_ms
	if now < 0.0:
		now = float(Time.get_ticks_msec())

	if render_tick == -INF:
		render_tick = float(snaps.back()["tick"])
		_last_sample_ms = now
	var step_ms := now - _last_sample_ms
	if step_ms <= 0.0:
		step_ms = 0.0
	if step_ms > tick_duration_ms * max_ticks_per_step:
		step_ms = tick_duration_ms * max_ticks_per_step
	_last_sample_ms = now

	var margin_ticks := mini(6, int(ceil(jitter_ms * 4.0 / tick_duration_ms)))
	if _margin < 0:
		_margin = margin_ticks
	elif margin_ticks != _margin and now - _margin_changed_ms > 2000.0:
		margin_ticks = _margin + 1 if margin_ticks > _margin else _margin - 1
		_margin = margin_ticks
		_margin_changed_ms = now
	else:
		margin_ticks = _margin
	var desired := interp_ticks + margin_ticks
	render_tick += float(desired - _target_ticks)
	_target_ticks = desired

	var step_ticks := step_ms / tick_duration_ms
	render_tick += step_ticks

	var latest: Dictionary = snaps.back()
	var target := render_tick - float(_target_ticks)

	if float(latest["tick"]) - target > float(_target_ticks + 3):
		render_tick += step_ticks * (speedup_factor - 1.0)
		_speedup_ticks += 1
		target = render_tick - float(_target_ticks)

	if float(latest["tick"]) - target > float(snap_ticks):
		render_tick = float(latest["tick"]) + float(_target_ticks)
		target = float(latest["tick"])
		var keep_from := 0
		for i in range(snaps.size()):
			if int(snaps[i]["tick"]) >= int(latest["tick"]) - _target_ticks:
				keep_from = i
				break
		if keep_from > 0:
			snaps = snaps.slice(keep_from)
		_snap_count += 1
	elif target > float(latest["tick"]):
		render_tick = float(latest["tick"]) + float(_target_ticks)
		target = float(latest["tick"])

	while snaps.size() >= 3 and float(snaps[1]["tick"]) <= target:
		snaps.pop_front()

	return _interpolate_at(target)


func target_tick() -> float:
	return render_tick - float(_target_ticks)


func stats() -> Dictionary:
	var newest := int(snaps.back()["tick"]) if not snaps.is_empty() else 0
	return {
		"buffered": snaps.size(),
		"tickDurationMs": tick_duration_ms,
		"jitterMs": jitter_ms,
		"renderTick": target_tick(),
		"speedups": _speedup_ticks,
		"snaps": _snap_count,
		"behind": float(newest) - target_tick(),
	}


func reset() -> void:
	snaps.clear()
	render_tick = -INF
	_last_sample_ms = -1.0
	_last_arrival_ms = -1.0
	jitter_ms = 0.0
	_margin = -1
	_target_ticks = interp_ticks
	_margin_changed_ms = 0.0


func _interpolate_at(target_tick_value: float) -> Variant:
	if snaps.is_empty():
		return null
	if target_tick_value >= float(snaps.back()["tick"]):
		return snaps.back()["entities"]
	if target_tick_value <= float(snaps[0]["tick"]):
		return snaps[0]["entities"]

	var index := snaps.size() - 1
	while index > 0 and float(snaps[index]["tick"]) > target_tick_value:
		index -= 1
	var a: Dictionary = snaps[index]
	var b: Dictionary = snaps[index + 1]
	var span := float(b["tick"]) - float(a["tick"])
	var alpha := 0.0 if span == 0.0 else (target_tick_value - float(a["tick"])) / span
	alpha = clampf(alpha, 0.0, 1.0)

	if lerp_fn.is_valid():
		return lerp_fn.call(a["entities"], b["entities"], alpha)

	if typeof(b["entities"]) != TYPE_DICTIONARY:
		return b["entities"]
	var out := {}
	var newer: Dictionary = b["entities"]
	var older: Dictionary = a["entities"]
	for entity_id in newer.keys():
		var e1: Variant = newer[entity_id]
		var e0: Variant = older.get(entity_id)
		if e0 == null or typeof(e1) != TYPE_ARRAY or typeof(e0) != TYPE_ARRAY:
			out[entity_id] = e1
			continue
		var blended := []
		blended.resize(e1.size())
		for k in range(e1.size()):
			if discrete.has(k):
				blended[k] = e1[k]
				continue
			var blend: Variant = field_lerp.get(k)
			if blend is Callable and (blend as Callable).is_valid():
				blended[k] = (blend as Callable).call(e0[k], e1[k], alpha)
			else:
				blended[k] = float(e0[k]) + (float(e1[k]) - float(e0[k])) * alpha
		out[entity_id] = blended
	return out
