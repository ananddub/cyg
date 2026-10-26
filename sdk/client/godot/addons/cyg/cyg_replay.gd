class_name CYGReplayPlayer
extends RefCounted
## Port of the JS `CYGReplayPlayer`: instant replay, kill-cam and match
## spectating from a recorded `.oxr` payload.
##
## The JS player drives itself with `requestAnimationFrame`; here `play()`
## awaits `SceneTree.process_frame`, so it also works headless.

var data: Dictionary
## `[{ "t": float, "bytes": PackedByteArray }]`
var frames: Array = []
var duration_ms := 0.0
var current_index := 0
var is_playing := false
var speed := 1.0
var clock := 0.0
var last_ticks := 0.0

var on_delta_callback: Callable = Callable()
var on_progress_callback: Callable = Callable()
var on_ended_callback: Callable = Callable()


func _init(replay_data: Dictionary = {}) -> void:
	data = replay_data
	var raw_frames: Array = replay_data.get("frames", [])
	for frame in raw_frames:
		if frame is Dictionary:
			var raw: Variant = frame.get("d", PackedByteArray())
			var bytes: PackedByteArray = PackedByteArray(raw) if raw is Array else raw
			frames.append({
				"t": float(frame.get("t", 0.0)),
				"bytes": bytes,
			})
	duration_ms = float(replay_data.get("durationMs", 0.0))
	if duration_ms == 0.0 and not frames.is_empty():
		duration_ms = float(frames[frames.size() - 1]["t"])


func play(on_delta: Callable = Callable()) -> void:
	if on_delta.is_valid():
		on_delta_callback = on_delta
	if is_playing:
		return
	if current_index >= frames.size():
		current_index = 0
		clock = 0.0
	is_playing = true
	last_ticks = _now_ms()
	_run()


func pause() -> void:
	is_playing = false


func seek(progress: float) -> void:
	var clamped := clampf(progress, 0.0, 1.0)
	var target_time := clamped * duration_ms
	clock = target_time
	var found := 0
	for i in range(frames.size()):
		if float(frames[i]["t"]) <= target_time:
			found = i
		else:
			break
	current_index = found
	if found < frames.size() and on_delta_callback.is_valid():
		on_delta_callback.call(frames[found]["bytes"], found, frames.size())
	if on_progress_callback.is_valid():
		on_progress_callback.call(clamped, clock, duration_ms)


func set_speed(value: float) -> void:
	speed = clampf(value, 0.1, 5.0)


func on_ended(callback: Callable) -> CYGReplayPlayer:
	on_ended_callback = callback
	return self


func on_progress(callback: Callable) -> CYGReplayPlayer:
	on_progress_callback = callback
	return self


func _now_ms() -> float:
	return float(Time.get_ticks_usec()) / 1000.0


func _run() -> void:
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		is_playing = false
		return
	while is_playing:
		await tree.process_frame
		if not is_playing:
			return
		var now := _now_ms()
		var dt := (now - last_ticks) * speed
		last_ticks = now
		clock += dt
		while current_index < frames.size() and float(frames[current_index]["t"]) <= clock:
			var frame: Dictionary = frames[current_index]
			if on_delta_callback.is_valid():
				on_delta_callback.call(frame["bytes"], current_index, frames.size())
			current_index += 1
		if on_progress_callback.is_valid():
			var progress := 1.0 if duration_ms <= 0.0 else minf(1.0, clock / duration_ms)
			on_progress_callback.call(progress, clock, duration_ms)
		if current_index >= frames.size():
			is_playing = false
			if on_ended_callback.is_valid():
				on_ended_callback.call()
			return
