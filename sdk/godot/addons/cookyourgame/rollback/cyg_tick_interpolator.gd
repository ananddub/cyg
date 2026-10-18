class_name CYGTickInterpolator
extends RefCounted
## Generic tick interpolator: the smoothing layer between the 30 Hz network
## tick and the display refresh rate (60/144/240 Hz).
##
## Each named track is a short ring of tick-stamped values; the interpolator
## evaluates them at an arbitrary render time, defaulting to
## `CYGNetworkTime.rollback_msec()` (one latency window behind), so incoming
## reconciliations land in history that has not been rendered yet and never
## produce a visible snap.
##
## This is the generic primitive the rollback layer uses for non-pose values
## (velocity magnitude, aim angle, animation phase, health bars). It does not
## replace CYGTransformSync3D, which hard-codes pose fields; tracks here are
## keyed by name instead.
##
## Design derivation: Netfox's tick interpolator (attribution lives in
## LICENSE-NETFOX.md). Clean-room GDScript; no Netfox source text is copied.


## Samples kept per track. 16 covers any reasonable jitter window at 30 Hz.
const RING_CAPACITY := 16

var _time: CYGNetworkTime = null
var _tracks: Dictionary = {}
var _extrapolation_limit: int = 2

## WHY: the hot path is one `sample_*` call per track per rendered frame; the
## last (key, track) pair is memoized so repeated samples skip the Dictionary
## hash entirely and never allocate.
var _last_key: String = ""
var _last_track: RollbackTrack = null


## `time` provides the tick<->millisecond mapping and the rollback clock that
## defines the default render time.
func _init(time: CYGNetworkTime) -> void:
	_time = time


## Registers an empty float track; re-creating an existing key replaces it and
## drops its history. Unknown keys are tolerated by every other call (samples
## return null), so creating ahead of pushing is always safe.
func create_float_track(key: String) -> void:
	_tracks[key] = FloatTrack.new()
	_last_track = null


func create_vector2_track(key: String) -> void:
	_tracks[key] = Vector2Track.new()
	_last_track = null


func create_vector3_track(key: String) -> void:
	_tracks[key] = Vector3Track.new()
	_last_track = null


func create_quaternion_track(key: String) -> void:
	_tracks[key] = QuaternionTrack.new()
	_last_track = null


## Appends a tick-stamped sample. Values are assumed to arrive in non-decreasing
## tick order; out-of-order or duplicate stamps bracket to holds, not errors.
func push_float(key: String, value_tick: int, value: float) -> void:
	var track: FloatTrack = _find_track(key) as FloatTrack
	if track == null:
		return
	track.push(value_tick, value)


func push_vector2(key: String, value_tick: int, value: Vector2) -> void:
	var track: Vector2Track = _find_track(key) as Vector2Track
	if track == null:
		return
	track.push(value_tick, value)


func push_vector3(key: String, value_tick: int, value: Vector3) -> void:
	var track: Vector3Track = _find_track(key) as Vector3Track
	if track == null:
		return
	track.push(value_tick, value)


func push_quaternion(key: String, value_tick: int, value: Quaternion) -> void:
	var track: QuaternionTrack = _find_track(key) as QuaternionTrack
	if track == null:
		return
	track.push(value_tick, value)


## Interpolates the track at the current rollback render time. Returns null for
## unknown or empty tracks, and when no CYGNetworkTime was provided.
func sample_float(key: String) -> Variant:
	if _time == null:
		return null
	return _sample_float_at(key, _time.rollback_msec())


func sample_vector2(key: String) -> Variant:
	if _time == null:
		return null
	return _sample_vector2_at(key, _time.rollback_msec())


func sample_vector3(key: String) -> Variant:
	if _time == null:
		return null
	return _sample_vector3_at(key, _time.rollback_msec())


func sample_quaternion(key: String) -> Variant:
	if _time == null:
		return null
	return _sample_quaternion_at(key, _time.rollback_msec())


## Interpolates at an explicit millisecond time (same clock epoch as
## `CYGNetworkTime.msec_from_tick`), for callers evaluating past render frames.
func sample_at_float(key: String, at_msec: float) -> Variant:
	return _sample_float_at(key, at_msec)


func sample_at_vector3(key: String, at_msec: float) -> Variant:
	return _sample_vector3_at(key, at_msec)


## Newest pushed value with no interpolation; null when the track is unknown
## or empty.
func last_value_float(key: String) -> Variant:
	var track: FloatTrack = _find_track(key) as FloatTrack
	if track == null:
		return null
	return track.last_or_null()


func last_value_vector3(key: String) -> Variant:
	var track: Vector3Track = _find_track(key) as Vector3Track
	if track == null:
		return null
	return track.last_or_null()


## Newest minus previous sample: a cheap "did it move since last tick" probe.
## 0.0 when the track is unknown, empty, or has a single sample.
func tick_diff_float(key: String) -> float:
	var track: FloatTrack = _find_track(key) as FloatTrack
	if track == null:
		return 0.0
	return track.tick_diff()


## How far past the newest sample interpolation may run before holding the
## newest value. Default 2 ticks; 0 disables extrapolation entirely.
func set_extrapolation_limit(ticks: int) -> void:
	_extrapolation_limit = maxi(ticks, 0)


## Drops a track's history but keeps its definition.
func clear_track(key: String) -> void:
	var track: RollbackTrack = _tracks.get(key)
	if track == null:
		return
	track.reset()
	if key == _last_key:
		_last_track = null


func clear_all() -> void:
	_last_track = null
	_last_key = ""
	for track in _tracks.values():
		var typed: RollbackTrack = track
		typed.reset()


func track_keys() -> PackedStringArray:
	return PackedStringArray(_tracks.keys())


func _find_track(key: String) -> RollbackTrack:
	if key == _last_key and _last_track != null:
		return _last_track
	var track: RollbackTrack = _tracks.get(key)
	if track == null:
		return null
	_last_key = key
	_last_track = track
	return track


func _sample_float_at(key: String, at_msec: float) -> Variant:
	var track: FloatTrack = _find_track(key) as FloatTrack
	if track == null:
		return null
	return track.evaluate(_tick_f(at_msec), _extrapolation_limit)


func _sample_vector2_at(key: String, at_msec: float) -> Variant:
	var track: Vector2Track = _find_track(key) as Vector2Track
	if track == null:
		return null
	return track.evaluate(_tick_f(at_msec), _extrapolation_limit)


func _sample_vector3_at(key: String, at_msec: float) -> Variant:
	var track: Vector3Track = _find_track(key) as Vector3Track
	if track == null:
		return null
	return track.evaluate(_tick_f(at_msec), _extrapolation_limit)


func _sample_quaternion_at(key: String, at_msec: float) -> Variant:
	var track: QuaternionTrack = _find_track(key) as QuaternionTrack
	if track == null:
		return null
	return track.evaluate(_tick_f(at_msec), _extrapolation_limit)


## Converts a millisecond time into the tick-number domain (fractional), using
## the clock's own mapping so tick epoch offsets are handled for free.
func _tick_f(at_msec: float) -> float:
	if _time == null:
		return 0.0
	var lower_tick: int = _time.tick_from_msec(at_msec)
	var lower_msec: float = _time.msec_from_tick(lower_tick)
	var upper_msec: float = _time.msec_from_tick(lower_tick + 1)
	var span: float = upper_msec - lower_msec
	if span <= 0.000001:
		return float(lower_tick)
	return float(lower_tick) + clampf((at_msec - lower_msec) / span, 0.0, 1.0)


## Ring of tick-stamped samples plus the bracket/extrapolation resolution
## shared by the typed subclasses. Subclasses own the parallel value ring.
class RollbackTrack extends RefCounted:
	const CAPACITY := 16
	const MASK := CAPACITY - 1

	const MODE_HOLD := 0
	const MODE_BRACKET := 1
	const MODE_EXTRAP := 2

	var ticks: Array[int] = []
	var head: int = 0
	var count: int = 0

	## Output of `_resolve`: the slots to combine and the combination factor.
	## MODE_HOLD reads `slot_a` only; MODE_BRACKET blends `slot_a` to `slot_b`
	## by `blend` in [0, 1]; MODE_EXTRAP treats `slot_a` as the previous sample
	## and `slot_b` as the newest, extending along their delta with
	## `blend` in [1, 1 + limit].
	var slot_a: int = 0
	var slot_b: int = 0
	var blend: float = 0.0
	var mode: int = MODE_HOLD

	func _init() -> void:
		for _i in range(CAPACITY):
			ticks.append(0)

	func reset() -> void:
		head = 0
		count = 0

	func _index(oldest_offset: int) -> int:
		return (head - count + oldest_offset) & MASK

	## False only when the track is empty; otherwise exactly one mode applies.
	func _resolve(tick_f: float, extrapolation_limit: int) -> bool:
		if count == 0:
			return false
		var newest: int = _index(count - 1)
		if tick_f >= float(ticks[newest]):
			if extrapolation_limit > 0 and count >= 2:
				mode = MODE_EXTRAP
				slot_a = _index(count - 2)
				slot_b = newest
				var excess: float = tick_f - float(ticks[newest])
				blend = 1.0 + clampf(excess, 0.0, float(extrapolation_limit))
			else:
				mode = MODE_HOLD
				slot_a = newest
			return true
		var oldest: int = _index(0)
		if tick_f <= float(ticks[oldest]):
			# Never invent history: hold the oldest sample.
			mode = MODE_HOLD
			slot_a = oldest
			return true
		for i in range(count - 1):
			var a: int = _index(i)
			var b: int = _index(i + 1)
			if tick_f < float(ticks[a]) or tick_f > float(ticks[b]):
				continue
			var span: float = float(ticks[b]) - float(ticks[a])
			if span <= 0.000001:
				mode = MODE_HOLD
				slot_a = b
				return true
			mode = MODE_BRACKET
			slot_a = a
			slot_b = b
			blend = (tick_f - float(ticks[a])) / span
			return true
		# Unreachable for monotonic stamps; hold the oldest rather than invent.
		mode = MODE_HOLD
		slot_a = oldest
		return true


class FloatTrack extends RollbackTrack:
	var values: Array[float] = []

	func _init() -> void:
		super._init()
		for _i in range(CAPACITY):
			values.append(0.0)

	func push(value_tick: int, value: float) -> void:
		ticks[head] = value_tick
		values[head] = value
		head = (head + 1) & MASK
		if count < CAPACITY:
			count += 1

	func last_or_null() -> Variant:
		if count == 0:
			return null
		return values[_index(count - 1)]

	func tick_diff() -> float:
		if count < 2:
			return 0.0
		return values[_index(count - 1)] - values[_index(count - 2)]

	func evaluate(tick_f: float, extrapolation_limit: int) -> Variant:
		if not _resolve(tick_f, extrapolation_limit):
			return null
		if mode == MODE_HOLD:
			return values[slot_a]
		return lerpf(values[slot_a], values[slot_b], blend)


class Vector2Track extends RollbackTrack:
	var values: Array[Vector2] = []

	func _init() -> void:
		super._init()
		for _i in range(CAPACITY):
			values.append(Vector2())

	func push(value_tick: int, value: Vector2) -> void:
		ticks[head] = value_tick
		values[head] = value
		head = (head + 1) & MASK
		if count < CAPACITY:
			count += 1

	func evaluate(tick_f: float, extrapolation_limit: int) -> Variant:
		if not _resolve(tick_f, extrapolation_limit):
			return null
		if mode == MODE_HOLD:
			return values[slot_a]
		return values[slot_a].lerp(values[slot_b], blend)


class Vector3Track extends RollbackTrack:
	var values: Array[Vector3] = []

	func _init() -> void:
		super._init()
		for _i in range(CAPACITY):
			values.append(Vector3())

	func push(value_tick: int, value: Vector3) -> void:
		ticks[head] = value_tick
		values[head] = value
		head = (head + 1) & MASK
		if count < CAPACITY:
			count += 1

	func last_or_null() -> Variant:
		if count == 0:
			return null
		return values[_index(count - 1)]

	func evaluate(tick_f: float, extrapolation_limit: int) -> Variant:
		if not _resolve(tick_f, extrapolation_limit):
			return null
		if mode == MODE_HOLD:
			return values[slot_a]
		return values[slot_a].lerp(values[slot_b], blend)


class QuaternionTrack extends RollbackTrack:
	var values: Array[Quaternion] = []

	func _init() -> void:
		super._init()
		for _i in range(CAPACITY):
			values.append(Quaternion())

	func push(value_tick: int, value: Quaternion) -> void:
		ticks[head] = value_tick
		values[head] = value
		head = (head + 1) & MASK
		if count < CAPACITY:
			count += 1

	func evaluate(tick_f: float, extrapolation_limit: int) -> Variant:
		if not _resolve(tick_f, extrapolation_limit):
			return null
		if mode == MODE_HOLD:
			return values[slot_a]
		# Shortest path: normalize the pair, negate when the dot product says
		# the long way round, then slerp (blend > 1 extends along the arc for
		# extrapolation).
		var from_q: Quaternion = values[slot_a].normalized()
		var to_q: Quaternion = values[slot_b].normalized()
		if from_q.dot(to_q) < 0.0:
			to_q = -to_q
		return from_q.slerp(to_q, blend)
