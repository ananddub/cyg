@tool
extends EditorPlugin
## CookYourGame addon entry. Registers the runtime autoloads when the plugin is
## enabled, so games only need to toggle the plugin to get `CYGNetwork`.

const AUTOLOAD_NETWORK := "CYGNetwork"
const NETWORK_SCRIPT := "res://addons/cookyourgame/core/cyg_network.gd"


func _enter_tree() -> void:
	if not _autoload_registered():
		add_autoload_singleton(AUTOLOAD_NETWORK, NETWORK_SCRIPT)


func _exit_tree() -> void:
	if _autoload_registered():
		remove_autoload_singleton(AUTOLOAD_NETWORK)


func _autoload_registered() -> bool:
	var configured := ProjectSettings.get_setting("autoload/" + AUTOLOAD_NETWORK, "")
	return configured == NETWORK_SCRIPT or configured == "*" + NETWORK_SCRIPT
