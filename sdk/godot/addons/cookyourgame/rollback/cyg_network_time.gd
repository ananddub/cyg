class_name CYGNetworkTime
extends RefCounted
## Drift-compensated network clock for the CookYourGame rollback layer.
##
## The host emits its tick from the render frame `_process`, so the counter it
## publishes always lags wall time by whatever a late frame stole. This clock runs
## its own wall-time expectation, re-anchors to the emitted counter, and exposes the
## tick a remote authority is probably still processing: `rollback_tick()`, which is
## where a predictor must rewind before resimulating.
##
## It mirrors the host's own tick counter and tick rate; it never invents a
## competing authoritative counter. The owner injects the emitted tick via
## `observe_tick()` and the per-frame time via `advance()`. No host type is
## referenced here: this is a plain clock the owner drives.
##
## Design derives from Netfox's MIT-licensed rollback engine (attribution in
## LICENSE-NETFOX.md). Clean-room GDScript; no Netfox source text is copied.

## The emitted counter is this many ticks off before we stop smoothing and hard
## re-anchor: 0..1 tick disagreement is frame jitter, 2+ means a real desync/reset.
const _HARD_RESYNC_TICKS := 2

## Per-observation weight of the exponential smoothing that absorbs sub-tick drift.
## Small enough that one late frame cannot teleport the prediction timeline.
const _SMOOTHING_FACTOR := 0.1

## Network ticks per second; the authority's cadence. Always floored at 1 below.
var ticks_per_second: int = 30

## Round-trip latency in milliseconds, owned by the caller; feeds `latency_ticks()`.
var latency_ms: float = 0.0

# Integer microsecond bookkeeping throughout: Godot exposes no unscaled double clock
# and accumulating float microseconds would bleed precision, so the wall clock is an
# int usec counter and only the derived tick divides back out.
var _elapsed_usec: int = 0
var _anchor_usec: int = 0
var _anchor_tick: int = 0
var _smoothing_offset_usec: int = 0
var _anchored: bool = false


func _init() -> void:
	reset()


## Length of one tick in microseconds. `ticks_per_second` is floored at 1 so a
## misconfigured clock can never divide by zero.
func tick_duration_usec() -> float:
	return 1000000.0 / float(maxi(ticks_per_second, 1))


## Length of one tick in milliseconds.
func tick_duration_ms() -> float:
	return 1000.0 / float(maxi(ticks_per_second, 1))


## Accumulates wall time. Call once per frame BEFORE `observe_tick()`, feeding the
## frame's elapsed milliseconds (same unit as `Time.get_ticks_msec()` deltas).
func advance(delta_ms: float) -> void:
	_elapsed_usec += roundi(delta_ms * 1000.0)


## Reconciles the wall-time expectation against the counter the host just
## emitted. A disagreement of `>= _HARD_RESYNC_TICKS` snaps the anchor to the
## counter (the counter is authoritative); anything smaller is frame jitter and only
## nudges the smoothing offset, so a single late frame cannot jump the prediction
## timeline but a reset/desync still corrects on the spot.
func observe_tick(tick: int) -> void:
	var drift: int = tick - tick()
	if not _anchored or absi(drift) >= _HARD_RESYNC_TICKS:
		_anchor_tick = tick
		_anchor_usec = _elapsed_usec
		_smoothing_offset_usec = 0
		_anchored = true
		return

	# 0..1 tick: ease the offset toward the drift expressed in microseconds.
	var target_usec: float = float(drift) * tick_duration_usec()
	var blended: float = float(_smoothing_offset_usec) \
		+ _SMOOTHING_FACTOR * (target_usec - float(_smoothing_offset_usec))
	_smoothing_offset_usec = roundi(blended)


## Drift-compensated current tick: the anchor plus however many whole ticks of wall
## time (offset-adjusted) have passed since it was observed.
func tick() -> int:
	var since_anchor_usec: int = _elapsed_usec - _anchor_usec
	var continuous: float = (float(since_anchor_usec) + float(_smoothing_offset_usec)) / tick_duration_usec()
	return _anchor_tick + floori(continuous)


## Wall time accumulated since `reset()`, in milliseconds.
func elapsed_ms() -> float:
	return float(_elapsed_usec) / 1000.0


## Maps a millisecond timestamp onto its tick index.
func tick_from_msec(ms: float) -> int:
	return floori(ms / tick_duration_ms())


## Maps a tick index back onto its millisecond timestamp.
func msec_from_tick(value_tick: int) -> float:
	return float(value_tick) * tick_duration_ms()


## How many whole ticks the current latency spans, floored at 0.
func latency_ticks() -> int:
	return maxi(0, roundi(latency_ms / tick_duration_ms()))


## The tick a remote authority is probably still processing; rewind target.
func rollback_tick() -> int:
	return tick() - latency_ticks()


## `rollback_tick()` rendered as a wall-clock millisecond timestamp.
func rollback_msec() -> float:
	return msec_from_tick(rollback_tick())


## 0..1 position inside the current tick; 0.0 just after a tick boundary, 1.0 right
## before the next one. Consumes the same smoothing offset as `tick()`.
func sub_tick_phase() -> float:
	var since_anchor_usec: int = _elapsed_usec - _anchor_usec
	var continuous: float = (float(since_anchor_usec) + float(_smoothing_offset_usec)) / tick_duration_usec()
	return clampf(continuous - floorf(continuous), 0.0, 1.0)


## Signed, smoothed wall-vs-counter drift in milliseconds. Positive means the wall
## expectation is ahead of the emitted counter and is being eased back.
func smoothing_offset_ms() -> float:
	return float(_smoothing_offset_usec) / 1000.0


## Drops back to a cold clock: zero elapsed wall time, no anchor, no smoothing.
func reset() -> void:
	_elapsed_usec = 0
	_anchor_usec = 0
	_anchor_tick = 0
	_smoothing_offset_usec = 0
	_anchored = false
