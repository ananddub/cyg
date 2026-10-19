@tool
extends Control
## Bottom-panel dock: live connection state and a one-click audit of the open scene.
##
## Two jobs, both aimed at the moment a developer is guessing. The top rows read the
## running `CYGNetwork` autoload — connected or not, which tick, what round trip, how
## many views replicate, how many deltas per second — so "it isn't syncing" is answered
## before anyone adds a print. The tree below lists what `CYGSceneScanner` found in the
## currently edited scene.
##
## Nothing here is referenced through the `EditorInterface` *type*: the SDK ships as an
## addon, and a script that names an editor-only class fails to parse in an exported
## game that happens to load this scene. The interface arrives as an `Object` from the
## plugin and is only ever used through duck-typed method calls.

const REFRESH_INTERVAL := 0.25

## Refresh throttle, in seconds of accumulated `_process` time.
var _accum := 0.0

@onready var _status: Label = %Status
@onready var _tick: Label = %Tick
@onready var _latency: Label = %Latency
@onready var _views: Label = %Views
@onready var _deltas: Label = %Deltas
@onready var _scan_button: Button = %Scan
@onready var _results: Tree = %Results

## The plugin's `EditorInterface`, held as an `Object` so this script still parses in a
## running game. Null outside the editor, which is why every use goes through
## `_edited_scene_root()`.
var editor_interface: Object = null


func _ready() -> void:
	_results.columns = 2
	_results.set_column_title(0, "Node")
	_results.set_column_title(1, "Finding")
	_results.set_column_titles_visible(true)
	_results.hide_root = true
	_scan_button.pressed.connect(_on_scan_pressed)
	_refresh()


## Repaints on a slow cadence rather than every frame: connection numbers move at a
## fraction of the frame rate, and a dock that repaints at 60 Hz makes itself the most
## expensive node in the editor.
func _process(delta: float) -> void:
	_accum += delta
	if _accum < REFRESH_INTERVAL:
		return
	_accum = 0.0
	_refresh()



## Repaints the live rows.
func _refresh() -> void:
	var network := _network()
	if network == null:
		_set_rows("CYGNetwork is not running", "-", "-", "-", "-")
		return
	var state := "Connecting…"
	if network.is_connected_flag:
		state = "Connected as actor %d%s" % [
			network.local_actor_number,
			" (master)" if network.is_master_client else "",
		]
	_set_rows(state, "%d" % network.current_tick, "%.1f ms" % network.latency_ms,
		"%d" % network.view_count,
		"%d out / %d in" % [network.deltas_sent_per_second, network.deltas_received_per_second])


func _set_rows(status: String, tick: String, latency: String, views: String, rates: String) -> void:
	if _status.text == status and _tick.text == tick and _latency.text == latency \
			and _views.text == views and _deltas.text == rates:
		return
	_status.text = status
	_tick.text = "Tick: %s" % tick
	_latency.text = "RTT: %s" % latency
	_views.text = "Views: %s" % views
	_deltas.text = "Deltas/s: %s" % rates


## Renders one scan run. An empty list still clears the tree: leaving the previous
## scene's findings above a "no issues" line is how a tool loses credibility.
func show_issues(issues: Array) -> void:
	_results.clear()
	var tree_root := _results.create_item()
	for issue in issues:
		var is_error: bool = int(issue[CYGSceneScanner._ISSUE_SEVERITY]) == CYGSceneScanner.SEVERITY_ERROR
		var item := _results.create_item(tree_root)
		item.set_text(0, str(issue[CYGSceneScanner._ISSUE_PATH]))
		item.set_text(1, "%s  %s" % ["ERROR" if is_error else "warn", str(issue[CYGSceneScanner._ISSUE_MESSAGE])])
		item.set_custom_color(1, Color(1.0, 0.45, 0.4) if is_error else Color(1.0, 0.85, 0.4))


func _on_scan_pressed() -> void:
	show_issues(CYGSceneScanner.scan(_edited_scene_root()))


## The scene the user is looking at, or `null` outside the editor. Reached dynamically
## so this script has no compile-time dependency on editor classes.
func _edited_scene_root() -> Node:
	if editor_interface == null or not editor_interface.has_method("get_edited_scene_root"):
		return null
	return editor_interface.call("get_edited_scene_root") as Node


func _network() -> Node:
	var tree := get_tree()
	if tree == null or tree.root == null:
		return null
	return tree.root.get_node_or_null("CYGNetwork")
