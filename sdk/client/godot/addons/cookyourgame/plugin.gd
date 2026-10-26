@tool
extends EditorPlugin
## CookYourGame addon entry. Registers the runtime autoloads when the plugin is
## enabled, so games only need to toggle the plugin to get `CYGNetwork`.

const AUTOLOAD_NETWORK := "CYGNetwork"
const NETWORK_SCRIPT := "res://addons/cookyourgame/core/cyg_network.gd"
const DOCK_SCENE := "res://addons/cookyourgame/editor/cyg_editor_dock.tscn"
const DOCK_TITLE := "CYG Network"

var _dock: Control = null


func _enter_tree() -> void:
	if not _autoload_registered():
		add_autoload_singleton(AUTOLOAD_NETWORK, NETWORK_SCRIPT)
	if _dock == null:
		_dock = load(DOCK_SCENE).instantiate()
		# `set()` and not a property write: the dock keeps its `Object`-typed handle so
		# the script never names `EditorInterface`, which would break exports.
		_dock.set("editor_interface", get_editor_interface())
		add_control_to_bottom_panel(_dock, DOCK_TITLE)
		_dock.hide()


func _exit_tree() -> void:
	if _dock != null:
		remove_control_from_bottom_panel(_dock)
		_dock.queue_free()
		_dock = null
	if _autoload_registered():
		remove_autoload_singleton(AUTOLOAD_NETWORK)


func _autoload_registered() -> bool:
	var configured := ProjectSettings.get_setting("autoload/" + AUTOLOAD_NETWORK, "")
	return configured == NETWORK_SCRIPT or configured == "*" + NETWORK_SCRIPT
