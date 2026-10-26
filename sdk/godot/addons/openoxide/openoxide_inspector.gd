class_name OpenOxideInspector
extends RefCounted
## Port of the JS `StateInspector`: a development overlay listing the current
## schema, session ids and entity state.
##
## Headless users can call `snapshot()` / `render()` (which returns the JSON
## text); `attach()` adds the floating panel to a running scene.

var room: Variant
var interval_ms := 250
var label: RichTextLabel = null
var _layer: CanvasLayer = null
var _attached := false
var _rendered := ""


func _init(target_room: Variant = null, inspector_options: Dictionary = {}) -> void:
	room = target_room
	interval_ms = int(inspector_options.get("intervalMs", 250))


func snapshot() -> Dictionary:
	var schema_info: Variant = null
	if room != null and room.schema != null:
		var fields: Array = []
		for field in room.schema.fields:
			fields.append({ "name": field.get("name", ""), "type": field.get("type", "") })
		schema_info = {
			"name": room.schema.name,
			"version": room.schema.version,
			"hash": "0x%x" % (int(room.schema.hash) & 0xFFFFFFFF),
			"fields": fields,
		}
	var entities: Array = []
	if room != null and room.state != null:
		for entity in room.state.values():
			entities.append(entity.to_dict() if entity != null and entity.has_method("to_dict") else entity)
	return {
		"schema": schema_info,
		"sessionId": room.session_id if room != null else null,
		"playerId": room.player_id if room != null else null,
		"ready": bool(room._schema_ready) if room != null else false,
		"error": (room._schema_error if room != null and not String(room._schema_error).is_empty() else null),
		"entities": entities,
	}


## Adds the overlay below `target` (defaults to the current scene root).
func attach(target: Node = null) -> OpenOxideInspector:
	if _attached:
		return self
	if target == null:
		var tree := Engine.get_main_loop() as SceneTree
		target = tree.root if tree != null else null
	if target == null:
		return self
	_layer = CanvasLayer.new()
	_layer.layer = 100
	var panel := PanelContainer.new()
	panel.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	panel.position = Vector2(-376, -360)
	panel.custom_minimum_size = Vector2(360, 340)
	label = RichTextLabel.new()
	label.custom_minimum_size = Vector2(340, 320)
	label.fit_content = false
	label.scroll_active = true
	label.add_theme_font_size_override("normal_font_size", 12)
	panel.add_child(label)
	_layer.add_child(panel)
	target.add_child(_layer)
	_attached = true
	render()
	_tick()
	return self


func detach() -> OpenOxideInspector:
	_attached = false
	if _layer != null:
		_layer.queue_free()
		_layer = null
	label = null
	return self


## Rebuilds the JSON view and pushes it to the overlay when attached.
func render() -> String:
	_rendered = JSON.stringify(snapshot(), "  ")
	if label != null:
		label.text = _rendered
	return _rendered


func text() -> String:
	return _rendered


func _tick() -> void:
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		return
	while _attached:
		await tree.create_timer(float(interval_ms) / 1000.0).timeout
		if not _attached:
			return
		render()
