class_name CYGTimeSync
extends RefCounted
## Server-timeline estimator, ported from `CYGTimeSync` in the JS SDK.
##
## Keeps the last `max_samples` `(arrival_ms, server_tick)` points, fits the
## tick length as the Theil-Sen median of consecutive slopes (clamped to
## +/-25 % of the nominal rate), and takes the intercept from the lowest
## arrival-side origin so late packets cannot drag the clock backwards.
## Purely passive: one sample per authoritative delta.

var max_samples := 64
var nominal_tick_rate := 128
var tick_rate := 128
var ms_per_tick := 1000.0 / 128.0
var tick_origin_ms := -1.0
var min_latency_ms := 0
var synced := false

var _pts: Array = []


func _init(samples: int = 64) -> void:
	max_samples = samples


func set_tick_rate(rate: float) -> void:
	if is_finite(rate) and rate > 0.0:
		nominal_tick_rate = rate


func add_sample(arrival_ms: float, server_tick: float) -> void:
	if not is_finite(arrival_ms) or not is_finite(server_tick):
		return
	if not _pts.is_empty() and server_tick <= float(_pts.back()["tick"]):
		# Same-tick retransmit refreshes arrival; an older tick is ignored.
		if server_tick == float(_pts.back()["tick"]):
			_pts[_pts.size() - 1] = {"t": arrival_ms, "tick": server_tick}
	else:
		_pts.append({"t": arrival_ms, "tick": server_tick})
	while _pts.size() > max_samples:
		_pts.pop_front()
	_fit()


func _fit() -> void:
	if _pts.is_empty():
		return
	ms_per_tick = 1000.0 / nominal_tick_rate
	tick_rate = nominal_tick_rate

	if _pts.size() >= 2:
		var slopes: Array = []
		for i in range(1, _pts.size()):
			var dt: float = float(_pts[i]["t"]) - float(_pts[i - 1]["t"])
			var dn: float = float(_pts[i]["tick"]) - float(_pts[i - 1]["tick"])
			if dt > 0.0 and dn > 0.0:
				slopes.append(dt / dn)
		if not slopes.is_empty():
			var med := _median(slopes)
			var nominal := 1000.0 / nominal_tick_rate
			ms_per_tick = maxf(nominal * 0.75, minf(nominal * 1.25, med))
			tick_rate = 1000.0 / ms_per_tick

	var best := INF
	for point in _pts:
		var origin := float(point["t"]) - float(point["tick"]) * ms_per_tick
		if origin < best:
			best = origin
	tick_origin_ms = best
	synced = true


func tick_to_time_ms(tick: float) -> float:
	if not synced:
		return tick * ms_per_tick
	return tick_origin_ms + tick * ms_per_tick


func time_to_tick(t: float) -> float:
	if not synced:
		return 0.0
	return (t - tick_origin_ms) / ms_per_tick


func clock_offset_ms(_now_ms: float) -> float:
	if not synced:
		return 0.0
	return -tick_origin_ms


func sample_count() -> int:
	return _pts.size()


func reset() -> void:
	_pts.clear()
	tick_origin_ms = -1.0
	synced = false


static func _median(values: Array) -> float:
	var n := values.size()
	if n == 0:
		return NAN
	var sorted_values := values.duplicate()
	sorted_values.sort()
	if n & 1:
		return float(sorted_values[(n - 1) >> 1])
	return (float(sorted_values[n / 2 - 1]) + float(sorted_values[n / 2])) * 0.5
