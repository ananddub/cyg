class_name CYGMultiplayerPeer
## Adapter that carries Godot's native packet stream through a CookYourGame room.
##
## Godot does not let a GDScript class implement `MultiplayerPeer`: its
## `put_packet`/`get_packet`/`poll` are native virtuals, and overriding them
## from script is rejected ("won't be called by the engine"). This adapter keeps
## the exact same surface, but it is driven explicitly instead of by
## `MultiplayerAPI`: `CYGNetwork` forwards every tunneled packet addressed to
## the Godot pipe (`view_id = 0`, hash `0xFFFF`) into `_on_godot_packet`, and the
## game calls `put_packet()` / `get_packet()` from its own tick.
##
## Wire layout inside the pipe (after the 7-byte RPC header the router strips):
##   [u16 BE sender actor][native packet bytes]
##
## Usage:
##     var peer := CYGMultiplayerPeer.new()
##     peer.create_client()          # after CYGNetwork joined the room
##     peer.put_packet(bytes)        # broadcast to the room
##     while peer.get_available_packet_count() > 0:
##         var data := peer.get_packet()
##         var from := peer.get_remote_sender_id()

signal peer_connected(peer_id: int)
signal peer_disconnected(peer_id: int)

const BROADCAST_TARGET := 0

var _unique_id := 1
var _incoming: Array[PackedByteArray] = []
var _incoming_senders: Array[int] = []
var _sender_of_current_packet := 0
var _actor_to_peer: Dictionary = {}
var _peer_to_actor: Dictionary = {}
var _next_peer_id := 2
var _attached := false


## Subscribes to the network signals; safe to call repeatedly.
func attach() -> void:
	if _attached:
		return
	if not CYGNetwork.godot_packet_received.is_connected(_on_godot_packet):
		CYGNetwork.godot_packet_received.connect(_on_godot_packet)
	if not CYGNetwork.player_joined.is_connected(_on_player_joined):
		CYGNetwork.player_joined.connect(_on_player_joined)
	if not CYGNetwork.player_left.is_connected(_on_player_left):
		CYGNetwork.player_left.connect(_on_player_left)
	_attached = true


## Detaches from the network signals and drops the queue.
func detach() -> void:
	if not _attached:
		return
	if CYGNetwork.godot_packet_received.is_connected(_on_godot_packet):
		CYGNetwork.godot_packet_received.disconnect(_on_godot_packet)
	if CYGNetwork.player_joined.is_connected(_on_player_joined):
		CYGNetwork.player_joined.disconnect(_on_player_joined)
	if CYGNetwork.player_left.is_connected(_on_player_left):
		CYGNetwork.player_left.disconnect(_on_player_left)
	_attached = false


## Registers the local actor as peer id 1. Call after the CYG join.
func create_client() -> int:
	attach()
	if not CYGNetwork.is_connected_flag:
		push_warning("CYGMultiplayerPeer.create_client: CYGNetwork is not connected yet.")
		return ERR_UNCONFIGURED
	_unique_id = 1
	_actor_to_peer[CYGNetwork.local_actor_number] = 1
	_peer_to_actor[1] = CYGNetwork.local_actor_number
	return OK


## Reserves a peer id for an actor without waiting for its first packet.
func peer_id_for_actor(actor: int) -> int:
	if _actor_to_peer.has(actor):
		return _actor_to_peer[actor]
	var id := _next_peer_id
	_next_peer_id += 1
	_actor_to_peer[actor] = id
	_peer_to_actor[id] = actor
	return id


func actor_for_peer_id(peer_id: int) -> int:
	return _peer_to_actor.get(peer_id, 0)


func get_unique_id() -> int:
	return _unique_id


## Hands a native packet to the room. The sender slot always carries the local
## actor; peers filter by their own addressing.
func put_packet(data: PackedByteArray) -> Error:
	attach()
	if not CYGNetwork.is_connected_flag:
		return ERR_UNCONFIGURED
	var writer := CYGBitPacker.new()
	# Fixed-width header: the router decodes bytes 0-6 before it hands us the rest.
	writer.write_uint32_be(CYGOpcodes.GODOT_PIPE_VIEW_ID)
	writer.write_uint16_be(CYGOpcodes.GODOT_PIPE_HASH)
	writer.write_byte(CYGOpcodes.GODOT_PIPE_TARGET)
	writer.write_uint16_be(CYGNetwork.local_actor_number & 0xFFFF)
	writer.write_raw(data)
	if not CYGNetwork.send_rpc_payload(writer.to_bytes()):
		return ERR_UNAVAILABLE
	return OK


func get_available_packet_count() -> int:
	return _incoming.size()


func get_packet() -> PackedByteArray:
	if _incoming.is_empty():
		return PackedByteArray()
	_sender_of_current_packet = _incoming_senders.pop_front()
	return _incoming.pop_front()


func get_remote_sender_id() -> int:
	return _sender_of_current_packet


func close() -> void:
	detach()
	_incoming.clear()
	_incoming_senders.clear()


func _on_godot_packet(packet: PackedByteArray) -> void:
	if packet.size() < 2:
		return
	var sender_peer := peer_id_for_actor(((int(packet[0]) << 8) | int(packet[1])))
	if sender_peer == _unique_id:
		return
	_enqueue(sender_peer, packet.slice(2))


func _on_player_joined(player: CYGPlayer) -> void:
	peer_connected.emit(peer_id_for_actor(player.actor_number))


func _on_player_left(player: CYGPlayer) -> void:
	var id := int(_actor_to_peer.get(player.actor_number, 0))
	if id > 0:
		peer_disconnected.emit(id)


func _enqueue(sender_peer: int, payload: PackedByteArray) -> void:
	_incoming.append(payload)
	_incoming_senders.append(sender_peer)
