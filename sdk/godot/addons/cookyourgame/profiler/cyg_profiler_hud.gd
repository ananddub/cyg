class_name CYGProfilerHUD
extends Control
## Self-contained in-game overlay for `CYGNetworkProfiler`.
##
## Instantiate `cyg_profiler_hud.tscn` (or add this `Control` under any canvas)
## and the panel fills itself from `CYGNetworkProfiler.snapshot()` a few times a
## second plus once per committed window via the `sample_taken` signal. Every
## visible node — the labels and the three `ProgressBar`s below — lives in the
## scene file and is bound with `@onready`; nothing is constructed or looked up
## dynamically, and the script never reads `Input`, so it runs unchanged under
## `--headless` (tests can call `update_now()` and read `current_snapshot`).
##
## Node contract inside `cyg_profiler_hud.tscn`:
##
##  * `Panel` — `PanelContainer` sized by its content.
##  * `Panel/Margin/Box` — `VBoxContainer` holding, in order: `TitleLabel`,
##    `TrafficLabel`, `PacketsLabel`, `RttLabel`, `TickLabel`, `RollbackLabel`,
##    `ViewsLabel`, `OutBar`, `InBar`, `TickBar`.
##
## Inert by design: with no linked profiler, or a profiler whose `enabled` is
## false, `_process` returns after one branch and shows the offline placeholder.
## Merely instantiating the scene does not create or enable a profiler, matching
## the rule that SDK startup ticks nothing.
##
## Usage:
##
##     var hud: CYGProfilerHUD = preload(
##         "res://addons/cookyourgame/profiler/cyg_profiler_hud.tscn").instantiate()
##     add_child(hud)                       # links CYGNetworkProfiler.active if set
##     hud.link(wrapper.profiler)           # or explicitly
##     hud.visible = true
##
## Design derives from Netfox's MIT-licensed profiler and Mirror's interest/transport metrics (attribution in LICENSE-NETFOX.md). Clean-room GDScript; no upstream source text is copied.

## Seconds between full label refreshes when no sample signal drives an earlier one.
@export var redraw_interval: float = 0.25
## Link `CYGNetworkProfiler.active` in `_ready()` when nothing was linked yet.
@export var auto_link: bool = true

@onready var _title_label: Label = $Panel/Margin/Box/TitleLabel
@onready var _traffic_label: Label = $Panel/Margin/Box/TrafficLabel
@onready var _packets_label: Label = $Panel/Margin/Box/PacketsLabel
@onready var _rtt_label: Label = $Panel/Margin/Box/RttLabel
@onready var _tick_label: Label = $Panel/Margin/Box/TickLabel
@onready var _rollback_label: Label = $Panel/Margin/Box/RollbackLabel
@onready var _views_label: Label = $Panel/Margin/Box/ViewsLabel
@onready var _out_bar: ProgressBar = $Panel/Margin/Box/OutBar
@onready var _in_bar: ProgressBar = $Panel/Margin/Box/InBar
@onready var _tick_bar: ProgressBar = $Panel/Margin/Box/TickBar

## The snapshot rendered at the last refresh, exposed for tests and docks.
var current_snapshot: Dictionary = {}

var _profiler: CYGNetworkProfiler = null
var _accum := 0.0


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	if _profiler == null and auto_link:
		link(CYGNetworkProfiler.active)
	if _profiler == null:
		_render_offline()


## Points the overlay at `profiler` (null unlinks). The `sample_taken` signal
## is bound here so a committed window redraws immediately, not up to
## `redraw_interval` later.
func link(profiler: CYGNetworkProfiler) -> void:
	if profiler == _profiler:
		return
	if _profiler != null and _profiler.sample_taken.is_connected(_on_sample_taken):
		_profiler.sample_taken.disconnect(_on_sample_taken)
	_profiler = profiler
	if _profiler != null:
		_profiler.sample_taken.connect(_on_sample_taken)
		update_now()
	else:
		current_snapshot = {}
		_render_offline()


func _process(delta: float) -> void:
	if _profiler == null or not _profiler.enabled or not visible:
		return
	_accum += delta
	if _accum < redraw_interval:
		return
	_accum = 0.0
	_profiler.pump_realtime()
	update_now()


## Take a snapshot and render it. Deliberately does not rotate the profiler's
## clock — `pump()` belongs to whoever drives time (the frame `_process` above,
## or `CYGProfilingTransport.poll()`), which keeps headless callers deterministic.
func update_now() -> void:
	if _profiler == null:
		return
	current_snapshot = _profiler.snapshot()
	_render(current_snapshot)


func _on_sample_taken() -> void:
	# Throttled by the interval accumulator so a catch-up burst of commits
	# after a stall cannot make the HUD allocate a snapshot per window.
	if visible:
		update_now()


func _render_offline() -> void:
	_title_label.text = "CYG Network Profiler"
	_traffic_label.text = "offline: no profiler linked"
	_packets_label.text = ""
	_rtt_label.text = ""
	_tick_label.text = ""
	_rollback_label.text = ""
	_views_label.text = ""
	_out_bar.value = 0.0
	_in_bar.value = 0.0
	_tick_bar.value = 0.0


func _render(snapshot: Dictionary) -> void:
	var live: Dictionary = snapshot["live"]
	var samples: Array = snapshot["samples"]
	var latest: Dictionary = samples[samples.size() - 1] if not samples.is_empty() else {}
	# Rates read from the last committed window (a finished measurement) and
	# fall back to the open one before anything has committed, so the panel
	# never blinks to zero at each second boundary.
	var window: Dictionary = latest if not latest.is_empty() else live
	var rtt: Dictionary = snapshot["rtt"]
	var tick: Dictionary = snapshot["tick"]
	var rollback: Dictionary = snapshot["rollback"]

	_title_label.text = "CYG Network Profiler"
	if not snapshot["enabled"]:
		_traffic_label.text = "profiler disabled"
		return

	_traffic_label.text = "out %.1f KB/s   in %.1f KB/s" % [
		_kb_per_second(int(window["bytes_sent"])), _kb_per_second(int(window["bytes_received"]))]
	_packets_label.text = "pkt %.0f/s out, %.0f/s in" % [
		float(window["packets_sent"]), float(window["packets_received"])]
	_rtt_label.text = "rtt %.1f ms (min %.1f avg %.1f max %.1f jit %.1f)" % [
		rtt["current_ms"], rtt["min_ms"], rtt["avg_ms"], rtt["max_ms"], rtt["jitter_ms"]]
	_tick_label.text = "tick %.1f Hz / %.0f Hz nominal, max %.1f ms" % [
		tick["hz"], tick["nominal_hz"], tick["max_tick_ms"]]
	_rollback_label.text = "mispredict %d, rewinds %d" % [
		rollback["mispredictions"], rollback["rewinds"]]
	_views_label.text = "top views: %s" % _top_views(snapshot["view_delta_bytes"])

	# The bars scale to the ring peak so the shape survives quiet and stormy
	# seconds alike; before any window commits they scale to the live counts.
	var peak := 1.0
	for committed in samples:
		peak = maxf(peak, float(committed["bytes_sent"]) + float(committed["bytes_received"]))
	_out_bar.max_value = peak
	_in_bar.max_value = peak
	_out_bar.value = minf(float(window["bytes_sent"]), peak)
	_in_bar.value = minf(float(window["bytes_received"]), peak)
	_tick_bar.max_value = 2.0
	_tick_bar.value = clampf(float(tick["ratio"]), 0.0, 2.0)


## Bytes-per-second in kilobytes, the unit every label in this panel uses.
static func _kb_per_second(bytes: int) -> float:
	return float(bytes) / 1024.0


func _top_views(view_delta: Dictionary) -> String:
	var out_table: Dictionary = view_delta["out"]
	if out_table.is_empty():
		return "-"
	var ids: Array = out_table.keys()
	ids.sort_custom(func(a, b): return int(out_table[a]) > int(out_table[b]))
	var parts: PackedStringArray = []
	for i in range(mini(3, ids.size())):
		parts.append("v%d %dB" % [ids[i], out_table[ids[i]]])
	return ", ".join(parts)
