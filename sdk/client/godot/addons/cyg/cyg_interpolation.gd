class_name CYGInterpolationBuffer
extends RefCounted
## Arrival-time snapshot buffer: `sample()` returns the state rendered
## `delay_ms` in the past, blended between the two surrounding snapshots.
## Port of `CYGInterpolationBuffer` from the JS SDK.

var lerp_fn: Callable
var delay_ms := 100.0
var snaps: Array = []
var max_snaps := 32


func _init(lerp: Callable = Callable(), delay: float = 100.0) -> void:
	lerp_fn = lerp
	delay_ms = delay


func push(state: Variant, arrival_time: float = -1.0) -> void:
	var at := arrival_time
	if at < 0.0:
		at = float(Time.get_ticks_msec())
	snaps.append({"t": at, "state": state})
	while snaps.size() > max_snaps:
		snaps.pop_front()


## Interpolated state at `now - delay_ms`; null before the first snapshot.
func sample(now: float = -1.0) -> Variant:
	if snaps.is_empty():
		return null
	var at := now
	if at < 0.0:
		at = float(Time.get_ticks_msec())
	var target := at - delay_ms
	var latest: Dictionary = snaps.back()
	if target >= float(latest["t"]):
		return latest["state"]
	var first: Dictionary = snaps[0]
	if target <= float(first["t"]):
		return first["state"]

	for i in range(snaps.size() - 1, 0, -1):
		var b: Dictionary = snaps[i]
		var a: Dictionary = snaps[i - 1]
		if target >= float(a["t"]):
			var span: float = float(b["t"]) - float(a["t"])
			if span == 0.0:
				span = 1.0
			var alpha := (target - float(a["t"])) / span
			if lerp_fn.is_valid():
				return lerp_fn.call(a["state"], b["state"], alpha)
			return b["state"]
	return first["state"]
