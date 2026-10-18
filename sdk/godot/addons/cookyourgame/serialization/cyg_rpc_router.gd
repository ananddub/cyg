class_name CYGRpcRouter
## Routes CUSTOM_RPC frames to Callables bound per `CYGView`.
##
## Wire envelope, byte-identical to the Unity SDK:
## `[viewId u32 BE][methodHash u16 BE][target u8][args...]` — the method hash
## is FNV-1a 16 of the method name, so an RPC sent from Godot executes a
## `[CYGRPC]` method of the same name on a Unity peer and vice versa.
##
## Godot has no runtime attribute discovery, so handlers register explicitly:
## a component calls `bind_rpc("shoot", _on_shoot)` in `_ready`, and the router
## feeds it the decoded argument array when a frame for its view arrives.

const HEADER_LENGTH := 7

static var _dispatch: Dictionary = {}  # view_id -> {hash -> Callable}
static var _hash_owners: Dictionary = {}  # hash -> method name (collision warning)


## FNV-1a 16-bit hash of an ASCII method name, matching Unity's `Fnv1a`.
static func fnv1a(name: String) -> int:
	var hash := 0x811C
	var raw := name.to_utf8_buffer()
	for i in range(raw.size()):
		hash ^= raw[i]
		hash = (hash * 0x011D) & 0xFFFF
	return hash


static func bind(view: CYGView, method_name: String, handler: Callable) -> void:
	if view == null or view.view_id <= 0:
		push_error("CYGRpcRouter.bind: view must exist before binding '%s'." % method_name)
		return
	var hash := fnv1a(method_name)
	if _hash_owners.has(hash) and _hash_owners[hash] != method_name:
		push_warning("CYGRpcRouter: hash collision between RPC '%s' and '%s' (0x%04X)."
			% [_hash_owners[hash], method_name, hash])
	else:
		_hash_owners[hash] = method_name
	var by_hash: Dictionary = _dispatch.get(view.view_id, {})
	by_hash[hash] = handler
	_dispatch[view.view_id] = by_hash


static func unbind(view: CYGView) -> void:
	if view != null:
		_dispatch.erase(view.view_id)


static func unbind_id(view_id: int) -> void:
	_dispatch.erase(view_id)


## Sends an RPC, then runs it locally when the target includes this client.
## Returns whether the frame reached the transport.
static func invoke(view: CYGView, method_name: String, target: int, args: Array) -> bool:
	if view == null:
		push_error("CYGRpcRouter.invoke: view is null for '%s'." % method_name)
		return false
	if not CYGNetwork.is_connected_flag:
		return false

	var hash := fnv1a(method_name)
	var writer := CYGBitPacker.new()
	# The header is fixed width: varint view ids would shift the hash and the
	# routing byte, and the server reads them as four raw big-endian bytes.
	writer.write_uint32_be(view.view_id)
	writer.write_uint16_be(hash)
	writer.write_byte(CYGOpcodes.server_target(target))
	write_rpc_arguments(writer, args)

	var payload := writer.to_bytes()
	if not CYGNetwork.send_rpc_payload(payload):
		return false

	if _should_echo_locally(view, target):
		_execute(view.view_id, hash, payload.slice(HEADER_LENGTH))
	return true


static func _should_echo_locally(view: CYGView, target: int) -> bool:
	match target:
		CYGOpcodes.Target.ALL:
			return true
		CYGOpcodes.Target.OWNER:
			return view.is_mine
		CYGOpcodes.Target.MASTER_CLIENT:
			return CYGNetwork.is_master_client
		_:
			return false


## Consumes the payload of an inbound CUSTOM_RPC frame (header included).
static func handle_received(payload: PackedByteArray) -> void:
	if payload.size() < HEADER_LENGTH:
		return
	var view_id := _be32(payload, 0)
	var hash := _be16(payload, 4)
	var target := payload[6]

	if view_id == CYGOpcodes.GODOT_PIPE_VIEW_ID and hash == CYGOpcodes.GODOT_PIPE_HASH:
		CYGNetwork.godot_packet_received.emit(payload.slice(HEADER_LENGTH))
		return

	if not _dispatch.has(view_id):
		return
	var by_hash: Dictionary = _dispatch[view_id]
	if not by_hash.has(hash):
		return
	# Owner-scoped in either namespace: this SDK writes the relay's 3, a Unity
	# peer writes its own 4. Anything else was broadcast by the relay.
	if target == CYGOpcodes.RPC_TARGET_OWNER or target == CYGOpcodes.Target.OWNER:
		var view := CYGNetwork.get_view(view_id)
		if view == null or not view.is_mine:
			return
	_execute(view_id, hash, payload.slice(HEADER_LENGTH))


static func _execute(view_id: int, hash: int, argument_bytes: PackedByteArray) -> void:
	var handler: Callable = _dispatch[view_id][hash]
	if not handler.is_valid():
		return
	var reader := CYGBitReader.new()
	reader.attach(argument_bytes)
	# A handler takes the decoded list as its single argument, so the array is
	# passed as one value rather than spread across the call.
	handler.call(read_rpc_arguments(reader))


## Packs an RPC argument list.
##
## The envelope through byte 6 is Unity's; the argument stream is not. A Unity
## `[CYGRPC]` invoker reads its parameters with static types and no prefix, so
## nothing there can describe a dynamically typed Godot `Array`. Godot writes one
## type tag per argument and reads until the payload is exhausted, which keeps
## the frame free of a count that a variadic handler cannot use.
static func write_rpc_arguments(writer: CYGBitPacker, args: Array) -> void:
	for arg in args:
		write_arg(writer, arg)


static func read_rpc_arguments(reader: CYGBitReader) -> Array:
	var args := []
	while reader.remaining > 0:
		var arg := read_arg(reader)
		if reader.last_read_failed:
			break
		args.append(arg)
	return args


## Packs a nested, fixed-length argument list; nested streams carry a count
## because "until exhausted" would consume the arguments that follow them.
static func write_args(writer: CYGBitPacker, args: Array) -> void:
	writer.write_uint32(args.size())
	for arg in args:
		write_arg(writer, arg)


static func write_arg(writer: CYGBitPacker, arg: Variant) -> void:
	match typeof(arg):
		TYPE_BOOL:
			writer.write_byte(1)
			writer.write_bool(arg)
		TYPE_INT:
			writer.write_byte(2)
			writer.write_int64(arg)
		TYPE_FLOAT:
			writer.write_byte(3)
			writer.write_single(arg)
		TYPE_STRING:
			writer.write_byte(4)
			writer.write_string(arg)
		TYPE_VECTOR2:
			writer.write_byte(5)
			writer.write_single(arg.x)
			writer.write_single(arg.y)
		TYPE_VECTOR3:
			writer.write_byte(6)
			writer.write_vector3(arg)
		TYPE_QUATERNION:
			writer.write_byte(7)
			writer.write_quaternion(arg)
		TYPE_COLOR:
			writer.write_byte(8)
			writer.write_single(arg.r)
			writer.write_single(arg.g)
			writer.write_single(arg.b)
			writer.write_single(arg.a)
		TYPE_PACKED_BYTE_ARRAY:
			writer.write_byte(9)
			writer.write_bytes(arg)
		TYPE_ARRAY:
			writer.write_byte(10)
			write_args(writer, arg)
		TYPE_DICTIONARY:
			writer.write_byte(11)
			writer.write_uint32(arg.size())
			for key in arg.keys():
				write_arg(writer, key)
				write_arg(writer, arg[key])
		_:
			# null and unsupported shapes travel as a typed void marker.
			writer.write_byte(0)


static func read_args(reader: CYGBitReader) -> Array:
	var count := reader.read_uint32()
	var args := []
	args.resize(count)
	for i in range(count):
		args[i] = read_arg(reader)
	return args


static func read_arg(reader: CYGBitReader) -> Variant:
	match reader.read_byte():
		1:
			return reader.read_bool()
		2:
			return reader.read_int64()
		3:
			return reader.read_single()
		4:
			return reader.read_string()
		5:
			return Vector2(reader.read_single(), reader.read_single())
		6:
			return reader.read_vector3()
		7:
			return reader.read_quaternion()
		8:
			return Color(reader.read_single(), reader.read_single(), reader.read_single(), reader.read_single())
		9:
			return reader.read_bytes(reader.read_uint32())
		10:
			return read_args(reader)
		11:
			var count := reader.read_uint32()
			var dict := {}
			for i in range(count):
				var key := read_arg(reader)
				dict[key] = read_arg(reader)
			return dict
		_:
			return null


static func _be16(data: PackedByteArray, at: int) -> int:
	if at + 1 >= data.size():
		return 0
	return (int(data[at]) << 8) | int(data[at + 1])


static func _be32(data: PackedByteArray, at: int) -> int:
	if at + 3 >= data.size():
		return 0
	return ((int(data[at]) << 24) | (int(data[at + 1]) << 16) | (int(data[at + 2]) << 8) | int(data[at + 3])) & 0xFFFFFFFF
