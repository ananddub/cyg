extends Node
class_name OpenOxideClient

## OpenOxide Gaming Client for Godot 4
## Sub-millisecond UDP & WebSocket multiplayer networking.

signal on_connected()
signal on_disconnected()
signal on_delta(entities: Array, bullets: Array, local_id: int)

var udp: PacketPeerUDP = PacketPeerUDP.new()
var is_connected: bool = false
var server_host: String = "127.0.0.1"
var server_port: int = 7778
var room_name: String = "game_room"
var is_spectator: bool = false
var local_player_id: int = 0

func connect_to_server(host: String = "127.0.0.1", port: int = 7778, room: String = "game_room", spectator: bool = false) -> void:
	server_host = host
	server_port = port
	room_name = room
	is_spectator = spectator

	var err = udp.connect_to_host(server_host, server_port)
	if err == OK:
		is_connected = true
		if is_spectator:
			# Send spectator handshake [0xFF, 'S', 'P', 'E', 'C']
			var spec_packet = PackedByteArray([0xFF, 0x53, 0x50, 0x45, 0x43])
			udp.put_packet(spec_packet)
		emit_signal("on_connected")
	else:
		push_error("[OpenOxide] Failed to connect to UDP host: %d" % err)

func _process(_delta: float) -> void:
	if not is_connected:
		return

	while udp.get_available_packet_count() > 0:
		var packet: PackedByteArray = udp.get_packet()
		_parse_delta(packet)

func send_input(flags: int, aim_yaw: float, aim_pitch: float, move_x: float, move_z: float) -> void:
	if not is_connected or is_spectator:
		return

	var sp = StreamPeerBuffer.new()
	sp.data_array = PackedByteArray()
	sp.put_u32(flags)
	sp.put_float(aim_yaw)
	sp.put_float(aim_pitch)
	sp.put_float(move_x)
	sp.put_float(move_z)

	udp.put_packet(sp.data_array)

func _parse_delta(bytes: PackedByteArray) -> void:
	if bytes.size() < 12:
		return

	var sp = StreamPeerBuffer.new()
	sp.data_array = bytes

	# Check for 6-byte Colyseus/engine-net framing (0xA5 "delta")
	if bytes.size() >= 6 and bytes[0] == 0xA5 and bytes[1] == 100: # 'd'
		sp.seek(6)

	local_player_id = sp.get_64()
	var player_count = sp.get_u32()
	var entities: Array = []

	for i in range(player_count):
		if sp.get_position() + 72 > bytes.size():
			break
		var id = sp.get_64()
		var x = sp.get_double()
		var y = sp.get_double()
		var z = sp.get_double()
		var yaw = sp.get_double()
		var pitch = sp.get_double()
		var hp = sp.get_double()
		var score = sp.get_double()
		var state = sp.get_u32()
		var _pad = sp.get_u32()

		entities.append({
			"id": id,
			"position": Vector3(x, y, z),
			"yaw": yaw,
			"pitch": pitch,
			"health": hp,
			"score": score,
			"state": state,
			"is_bot": id < 0
		})

	var bullets: Array = []
	if sp.get_position() + 4 <= bytes.size():
		var bullet_count = sp.get_u32()
		for i in range(bullet_count):
			if sp.get_position() + 32 > bytes.size():
				break
			var bx = sp.get_double()
			var by = sp.get_double()
			var bz = sp.get_double()
			var owner_id = sp.get_64()
			bullets.append({
				"position": Vector3(bx, by, bz),
				"owner_id": owner_id
			})

	emit_signal("on_delta", entities, bullets, local_player_id)

func disconnect_from_server() -> void:
	if is_connected:
		udp.close()
		is_connected = false
		emit_signal("on_disconnected")
