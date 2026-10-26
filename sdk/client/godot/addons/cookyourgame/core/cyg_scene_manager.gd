class_name CYGSceneManager
extends Node
## Room-wide scene switch: the master client announces a scene path and every
## peer loads it on the same network tick, so nobody runs a different level.
##
## A scene change is a room event, not an RPC: it belongs to the room rather than
## to a view, and the object that requests it may be freed by the very switch it
## triggers. `CYGNetwork.send_room_event()` frames it as ROOM_DATA_BYTES (9)
## tunneled under wire 17, which the Rust relay multicasts to every other peer
## (the sender is excluded, so this node applies its own announcement locally).
##
## Event name `"CYG_SCENE"`; the body, byte for byte after the name block that
## `send_room_event` writes:
## `[LEB128 path length][utf8 path]` the `res://` .tscn path — the identity
## `[byte mode]`                0 = single, 1 = additive (`LoadSceneMode` of the
##                              Unity peer this layout is shared with)
## `[LEB128 u32 lead_ticks]`    switch this many network ticks after receipt
## `[LEB128 u32 stamp]`         `(actor << 20) | (tick & 0xFFFFF)`, the dedup key
## `[byte flags]`               bit0 = JOIN_SYNC (apply now, no tick boundary)
## The last three fields are optional on read: a Unity announcement is only
## `[string][byte]`, and a peer that cannot find them applies it immediately.
##
## `"CYG_SCENE_QUERY"` (empty body) asks whoever is master client to re-announce
## its current scene, which is how a late joiner — or a peer that reconnects
## mid-session — learns where the room actually is.
##
## Godot rejects a `class_name` that shadows an autoload, so this is a plain node
## parked under the `CYGNetwork` singleton instead: `CYGSceneManager.ensure_installed(CYGNetwork)`
## from boot code (or the one-line call in `CYGNetwork._ready`), which also keeps
## it out of the scene that a switch frees.

signal scene_change_requested(scene_path: String)
signal scene_changed(scene_path: String)
signal scene_change_failed(scene_path: String, reason: String)

const SCENE_EVENT_NAME := "CYG_SCENE"
const SCENE_QUERY_EVENT_NAME := "CYG_SCENE_QUERY"
const SINGLETON_NAME := "CYGSceneManager"

## `LoadSceneMode` values as the Unity SDK writes them.
const MODE_SINGLE := 0
const MODE_ADDITIVE := 1
const FLAG_JOIN_SYNC := 1

const ACTOR_SHIFT := 20
const ACTOR_MASK := 0x3FFF
const TICK_MASK := (1 << ACTOR_SHIFT) - 1
## Scene changes are rare enough that eight announcements are plenty of window
## for a relay retransmit; a fixed ring allocates nothing.
const DEDUP_RING := 8
const DEDUP_MASK := DEDUP_RING - 1
## `change_scene_to_file` is deferred, so the switch lands at the end of the
## frame `switch_lead_ticks` after the announcement was received.
const MIN_LEAD_TICKS := 1

## Re-announce (as master) or ask (as client) after a join or a rejoin.
@export var auto_restore: bool = true
## Only the master client may drive the room's scene, matching Unity's guard.
@export var require_master: bool = true
## Ticks between receiving an announcement and switching; small so the room still
## switches together, non-zero so a peer that is mid-frame does not cut a load in
## half.
@export_range(0, 30) var switch_lead_ticks: int = 2

var _current_path: String = ""
var _pending_path: String = ""
## The scene currently being loaded: `_pending_path` is cleared the moment the tick
## boundary fires, and the tree's `scene_changed` signal arrives one emit later
## with no argument to identify what was loaded.
var _switching_path: String = ""
var _switch_at_tick := 0
var _joined_already := false
var _seen: Array[int] = []
var _seen_cursor := 0


func _ready() -> void:
	_seen.resize(DEDUP_RING)
	# Connect first: an announcement could land on the very frame this node readies.
	if not CYGNetwork.room_event.is_connected(_on_room_event):
		CYGNetwork.room_event.connect(_on_room_event)
	if not CYGNetwork.network_tick.is_connected(_on_network_tick):
		CYGNetwork.network_tick.connect(_on_network_tick)
	if not CYGNetwork.joined_room.is_connected(_on_joined_room):
		CYGNetwork.joined_room.connect(_on_joined_room)
	if not CYGNetwork.player_joined.is_connected(_on_player_joined):
		CYGNetwork.player_joined.connect(_on_player_joined)
	if not CYGNetwork.disconnected.is_connected(_on_disconnected):
		CYGNetwork.disconnected.connect(_on_disconnected)
	var tree := get_tree()
	if tree != null and not tree.scene_changed.is_connected(_on_tree_scene_changed):
		tree.scene_changed.connect(_on_tree_scene_changed)
	# `SceneTree.change_scene_to_file()` frees the outgoing scene, so a manager a
	# game dropped into a level must not die with it: park it under the autoload.
	# Anywhere under the current scene counts, not just a direct child.
	# Deferred because `_ready` runs while the tree is still adding children, and
	# `reparent()` refuses then.
	if tree != null and tree.current_scene != null \
			and tree.current_scene.is_ancestor_of(self) and is_instance_valid(CYGNetwork):
		call_deferred("_park_under_network")
	# An autoload readies before a game's own scene, so a room already joined (a
	# reconnect through `CYGNetwork.reconnect_room`) is caught here too.
	if CYGNetwork.is_connected_flag:
		_on_joined_room(CYGNetwork.room_name)


func _park_under_network() -> void:
	if is_instance_valid(CYGNetwork) and is_inside_tree() and get_parent() != CYGNetwork:
		reparent(CYGNetwork)


## ----------------------------------------------------------------- public API

## Loads `path` on every peer. Master-client only unless `require_master` is off.
func request_scene_change(scene_path: String, lead_ticks: int = -1) -> bool:
	if scene_path.is_empty():
		push_error("CYGSceneManager.request_scene_change: empty scene path.")
		return false
	if not CYGNetwork.is_connected_flag:
		push_warning("CYGSceneManager.request_scene_change: not connected, '%s' not announced." % scene_path)
		return false
	if require_master and not CYGNetwork.is_master_client:
		push_warning("CYGSceneManager: only the master client may change the room's scene; this peer is a client.")
		return false
	# WHY check before sending: a path the authority cannot load would leave every
	# peer scheduled for a scene that only fails, so the request dies here instead.
	if not ResourceLoader.exists(scene_path, "PackedScene"):
		scene_change_failed.emit(scene_path, "missing_resource")
		push_warning("CYGSceneManager: '%s' is not a loadable PackedScene; nothing was announced." % scene_path)
		return false
	var lead := clampi(lead_ticks if lead_ticks >= 0 else switch_lead_ticks, 0, 30)
	_announce(scene_path, lead, 0)
	_schedule(scene_path, lead, 0)
	return true


## Asks the master client for the room's current scene (late join, cold boot).
func request_scene_sync() -> bool:
	if not CYGNetwork.is_connected_flag:
		return false
	return CYGNetwork.send_room_event(SCENE_QUERY_EVENT_NAME)


## The scene this room agreed on: the last announced path, or the engine's truth
## when this node has never seen an announcement.
func current_scene_path() -> String:
	if not _current_path.is_empty():
		return _current_path
	var scene := get_tree().current_scene
	return scene.scene_file_path if scene != null else ""


## The scene scheduled for the next tick boundary, or "" when none is pending.
func pending_scene_path() -> String:
	return _pending_path


## Installs the manager under `host` (typically the `CYGNetwork` autoload) if it
## is not there already, for projects that did not register the autoload.
static func ensure_installed(host: Node) -> CYGSceneManager:
	var existing := host.get_node_or_null(SINGLETON_NAME) as CYGSceneManager
	if existing != null:
		return existing
	var created := CYGSceneManager.new()
	created.name = SINGLETON_NAME
	host.add_child(created)
	return created


## ------------------------------------------------------------------- internals

func _announce(scene_path: String, lead_ticks: int, flags: int) -> void:
	var writer := CYGBitPacker.new()
	writer.write_string(scene_path)
	writer.write_byte(MODE_SINGLE)
	writer.write_uint32(maxi(lead_ticks, 0))
	writer.write_uint32(_make_stamp())
	writer.write_byte(flags)
	CYGNetwork.send_room_event(SCENE_EVENT_NAME, writer.to_bytes())


func _make_stamp() -> int:
	return ((CYGNetwork.local_actor_number & ACTOR_MASK) << ACTOR_SHIFT) \
		| (CYGNetwork.current_tick & TICK_MASK)


func _on_room_event(event_name: String, body: PackedByteArray) -> void:
	if event_name == SCENE_QUERY_EVENT_NAME:
		_answer_query_if_master()
		return
	if event_name != SCENE_EVENT_NAME or body.is_empty():
		return

	var reader := CYGBitReader.new()
	reader.attach(body)
	var scene_path := reader.read_string()
	if scene_path.is_empty():
		return
	var mode := MODE_SINGLE
	if reader.remaining > 0:
		mode = reader.read_byte()
	var lead_ticks := 0
	if reader.remaining > 0:
		lead_ticks = int(reader.read_uint32())
	var stamp := 0
	if reader.remaining > 0:
		stamp = int(reader.read_uint32())
	var flags := 0
	if reader.remaining > 0:
		flags = reader.read_byte()

	# A stamp of 0 means the announcement came from a peer that does not carry one
	# (the Unity SDK), so path equality is the only idempotence available.
	if stamp != 0 and _is_duplicate(stamp):
		return
	if mode == MODE_ADDITIVE:
		push_warning("CYGSceneManager: additive mode is outside the Godot SDK's single-scene switch; '%s' ignored." % scene_path)
		return
	_schedule(scene_path, maxi(lead_ticks, MIN_LEAD_TICKS), flags)


func _schedule(scene_path: String, lead_ticks: int, flags: int) -> void:
	if flags & FLAG_JOIN_SYNC != 0 and scene_path == current_scene_path():
		# A catch-up announcement that every peer in the room receives: only the one
		# that is out of place has to act, the rest must not reload the scene they
		# are already standing in.
		_current_path = scene_path
		return
	if scene_path == _pending_path:
		# Already scheduled; a retransmit must not push the boundary back.
		return
	_pending_path = scene_path
	if flags & FLAG_JOIN_SYNC != 0 or lead_ticks <= 0:
		# A joiner has no simulation running yet and a Unity peer sends no lead:
		# switch on the next tick boundary rather than waiting for one.
		_switch_at_tick = CYGNetwork.current_tick
	else:
		_switch_at_tick = CYGNetwork.current_tick + maxi(lead_ticks, MIN_LEAD_TICKS)
	scene_change_requested.emit(scene_path)


func _on_network_tick(tick: int) -> void:
	if _pending_path.is_empty() or tick < _switch_at_tick:
		return
	var scene_path := _pending_path
	_pending_path = ""
	_switch_at_tick = 0
	_perform_switch(scene_path)


func _perform_switch(scene_path: String) -> void:
	if not ResourceLoader.exists(scene_path, "PackedScene"):
		# WHY not a retry: the room already agreed on this scene, and hanging here
		# would leave this peer idle while its peers play. Stay where we are,
		# report it, and let the game decide.
		scene_change_failed.emit(scene_path, "missing_resource")
		push_warning("CYGSceneManager: '%s' arrived from the room but is not in this build; staying in place." % scene_path)
		return
	_switching_path = scene_path
	var error := get_tree().change_scene_to_file(scene_path)
	if error != OK:
		_switching_path = ""
		scene_change_failed.emit(scene_path, "change_scene_to_file returned %d" % error)
		push_warning("CYGSceneManager: change_scene_to_file('%s') failed with %d." % [scene_path, error])


func _on_tree_scene_changed() -> void:
	# Godot 4.7 emits `SceneTree.scene_changed` with no argument, so the new scene is
	# read back off the tree. Its own path wins over the announced one: a build that
	# repacks levels can load the same scene from a different `res://` path.
	var scene := get_tree().current_scene
	var loaded := "" if scene == null else scene.scene_file_path
	_current_path = loaded if not loaded.is_empty() else _switching_path
	_switching_path = ""
	_pending_path = ""
	_switch_at_tick = 0
	scene_changed.emit(_current_path)


func _on_joined_room(_room_name: String) -> void:
	if _joined_already:
		return
	_joined_already = true
	if not auto_restore:
		return
	if not _current_path.is_empty() and CYGNetwork.is_master_client:
		# Survived a reconnect: the room may have missed our scene while we were
		# gone, so re-announce it for everyone including ourselves.
		_announce(_current_path, 0, FLAG_JOIN_SYNC)
		_schedule(_current_path, 0, FLAG_JOIN_SYNC)
		return
	request_scene_sync()


func _on_disconnected(_reason: String) -> void:
	_joined_already = false
	# The tick counter restarts with the next seat, so a boundary computed against
	# the old one would fire immediately or never.
	_switch_at_tick = 0
	_pending_path = ""
	_switching_path = ""
	_seen.fill(0)
	_seen_cursor = 0


## Answers a peer that asked where the room is. Only the master owns that answer,
## so a room of clients produces at most one announcement per query.
func _answer_query_if_master() -> void:
	if _current_path.is_empty() or not CYGNetwork.is_master_client:
		return
	_announce(_current_path, 0, FLAG_JOIN_SYNC)


## Pushes the room's scene to a newcomer: the relay excludes a sender from its own
## room event, so a join announcement never reaches the peer that needs it.
func _on_player_joined(_player: CYGPlayer) -> void:
	if _current_path.is_empty() or not CYGNetwork.is_master_client:
		return
	_announce(_current_path, 0, FLAG_JOIN_SYNC)


func _is_duplicate(stamp: int) -> bool:
	for i in range(_seen.size()):
		if _seen[i] == stamp:
			return true
	_seen[_seen_cursor] = stamp
	_seen_cursor = (_seen_cursor + 1) & DEDUP_MASK
	return false
