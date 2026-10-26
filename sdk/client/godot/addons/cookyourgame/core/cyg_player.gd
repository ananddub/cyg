class_name CYGPlayer
## A participant in the room, local or remote.
##
## `actor_number` is the short id used by view ids and ownership; it is derived
## from the session id, mirroring the Unity SDK, because the relay room does not
## publish the guest-side player ids schema-free.

var actor_number: int = 0
var player_id: String = ""
var nickname: String = ""
var is_local: bool = false
var is_master_client: bool = false
var custom_properties: Dictionary = {}
var latency_ms: float = 0.0


func _init(p_actor: int = 0, p_nickname: String = "") -> void:
	actor_number = p_actor
	nickname = p_nickname


func get_property(key: String, fallback: Variant = null) -> Variant:
	return custom_properties.get(key, fallback)


func set_property(key: String, value: Variant) -> void:
	custom_properties[key] = value


func _to_string() -> String:
	return "[CYGPlayer %d '%s' %s]" % [actor_number, nickname, player_id]
