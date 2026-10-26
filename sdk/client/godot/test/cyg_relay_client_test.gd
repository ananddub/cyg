# Offline conformance test for the CYG Godot addon.
#
#   godot --headless --path sdk/godot --import
#   godot --headless --path sdk/godot -s res://test/cyg_relay_client_test.gd
#
# Runs without a server: schema/hash/codecs are checked against golden values
# produced by `sdk/web/cyg.js`, and room/schema/delta paths are driven by
# synthetic frames.
extends SceneTree

const PLAYER_HASH_GOLDEN := 0x44D746FC

var checks := 0
var failures := 0


class Buf:
	var bytes := PackedByteArray()

	func u8(v: int) -> void:
		bytes.append(v & 0xFF)

	func u16(v: int) -> void:
		u8(v & 0xFF)
		u8((v >> 8) & 0xFF)

	func u32(v: int) -> void:
		u16(v & 0xFFFF)
		u16((v >> 16) & 0xFFFF)

	func u64(v: int) -> void:
		u32(v & 0xFFFFFFFF)
		u32((v >> 32) & 0xFFFFFFFF)

	func f64(v: float) -> void:
		var at := bytes.size()
		bytes.resize(at + 8)
		bytes.encode_double(at, v)

	func s16(v: String) -> void:
		var raw := v.to_utf8_buffer()
		u16(raw.size())
		bytes.append_array(raw)

	func magic() -> void:
		bytes.append_array(PackedByteArray([79, 88, 83, 50]))


func check(label: String, ok: bool) -> void:
	checks += 1
	if not ok:
		failures += 1
	print(("CHECK " if ok else "FAIL  ") + label)


func player_schema() -> CYGSchemaDef:
	return CYGSchemaDef.new("Player", 1).f64("x").u8("hp").u32("tag")


func root_schema() -> CYGSchemaDef:
	return CYGSchemaDef.new("State", 1).u16("round")


func descriptor_bytes() -> PackedByteArray:
	return CYGSchemaDef.build_descriptor_v2("State", root_schema(),
		[{"name": "players", "schema": player_schema()}])


func full_section(rows: Array) -> PackedByteArray:
	var player := player_schema()
	var s := Buf.new()
	s.magic()
	s.u8(1)
	s.u32(player.hash)
	s.u32(rows.size())
	for row in rows:
		s.u32(int(row[0]))
		s.f64(float(row[1]))
		s.u8(int(row[2]))
		s.u32(int(row[3]))
	return s.bytes


func world_full(round_value: int, rows: Array) -> PackedByteArray:
	var f := Buf.new()
	f.magic()
	f.u8(3)
	f.u32(root_schema().hash)
	f.u16(1)
	f.u16(round_value)
	f.u16(1)
	var section := full_section(rows)
	f.u16(0)
	f.u32(player_schema().hash)
	f.u32(section.size())
	f.bytes.append_array(section)
	return f.bytes


func patch_section(update_id: int, x: float, remove_id: int) -> PackedByteArray:
	var p := Buf.new()
	p.magic()
	p.u8(2)
	p.u32(player_schema().hash)
	p.u32(2)
	p.u8(3)  # UPDATE
	p.u32(update_id)
	p.u32(1)  # field mask: x
	p.f64(x)
	p.u8(2)  # REMOVE
	p.u32(remove_id)
	return p.bytes


func world_patch(round_value: int, section: PackedByteArray) -> PackedByteArray:
	var f := Buf.new()
	f.magic()
	f.u8(4)
	f.u32(root_schema().hash)
	f.u16(1)
	f.u32(1)  # root mask: round
	f.u16(round_value)
	f.u16(1)
	f.u16(0)
	f.u32(player_schema().hash)
	f.u32(section.size())
	f.bytes.append_array(section)
	return f.bytes


func update_x_section(update_id: int, x: float) -> PackedByteArray:
	var p := Buf.new()
	p.magic()
	p.u8(2)
	p.u32(player_schema().hash)
	p.u32(1)
	p.u8(3)  # UPDATE
	p.u32(update_id)
	p.u32(1)  # field mask: x
	p.f64(x)
	return p.bytes


func projectile_schema() -> CYGSchemaDef:
	return CYGSchemaDef.new("Projectile", 1).f64("x").u32("owner")


func multi_descriptor() -> PackedByteArray:
	return CYGSchemaDef.build_descriptor_v2("State", root_schema(), [
		{"name": "players", "schema": player_schema()},
		{"name": "bullets", "schema": projectile_schema()},
	])


func projectile_full_section(rows: Array) -> PackedByteArray:
	var s := Buf.new()
	s.magic()
	s.u8(1)
	s.u32(projectile_schema().hash)
	s.u32(rows.size())
	for row in rows:
		s.u32(int(row[0]))
		s.f64(float(row[1]))
		s.u32(int(row[2]))
	return s.bytes


func multi_world_full(round_value: int, players: Array, bullets: Array) -> PackedByteArray:
	var f := Buf.new()
	f.magic()
	f.u8(3)
	f.u32(root_schema().hash)
	f.u16(1)
	f.u16(round_value)
	f.u16(2)
	var player_bytes := full_section(players)
	f.u16(0)
	f.u32(player_schema().hash)
	f.u32(player_bytes.size())
	f.bytes.append_array(player_bytes)
	var bullet_bytes := projectile_full_section(bullets)
	f.u16(1)
	f.u32(projectile_schema().hash)
	f.u32(bullet_bytes.size())
	f.bytes.append_array(bullet_bytes)
	return f.bytes


func multi_world_patch(round_value: int, collection_id: int,
		section: PackedByteArray) -> PackedByteArray:
	var f := Buf.new()
	f.magic()
	f.u8(4)
	f.u32(root_schema().hash)
	f.u16(1)
	f.u32(1 if round_value >= 0 else 0)
	if round_value >= 0:
		f.u16(round_value)
	f.u16(1)
	var collection_schema := player_schema() if collection_id == 0 else projectile_schema()
	f.u16(collection_id)
	f.u32(collection_schema.hash)
	f.u32(section.size())
	f.bytes.append_array(section)
	return f.bytes


func projectile_add_section(entity_id: int, x: float, owner: int) -> PackedByteArray:
	var p := Buf.new()
	p.magic()
	p.u8(2)
	p.u32(projectile_schema().hash)
	p.u32(1)
	p.u8(1)  # ADD
	p.u32(entity_id)
	p.f64(x)
	p.u32(owner)
	return p.bytes


func delta_payload(session: int, acked: int, tick: int,
		section: PackedByteArray) -> PackedByteArray:
	var out := Buf.new()
	out.u64(session)
	out.bytes.append_array(section)
	out.u16(acked)
	out.u32(tick)
	return out.bytes


func _initialize() -> void:
	test_schema_and_descriptor()
	test_codec_golden()
	test_sequencer()
	test_room_schema_and_delta()
	test_callbacks_and_multi_world()
	test_timesync_and_interpolation()
	test_rollback_and_voice()
	test_client_helpers()
	test_api_surface()
	test_replay_and_inspector()
	print("checks=%d failures=%d" % [checks, failures])
	quit(1 if failures > 0 else 0)


func test_schema_and_descriptor() -> void:
	var schema := CYGSchemaDef.new("Player", 1).f64("x").f64("y").u32("flags")
	check("schema hash matches JS golden", schema.hash == PLAYER_HASH_GOLDEN)
	check("index_of", schema.index_of("y") == 1 and schema.index_of("nope") == -1)

	var optional := CYGSchemaDef.new("Thing", 1).u32("x").optional()
	check("optional flag", bool(optional.fields[0]["optional"]))
	var full_only := CYGSchemaDef.new("Thing", 1).u32("x").static_field()
	check("static is full-only", bool(full_only.fields[0]["full_only"]))

	var definition := CYGSchemaDef.new("Bad", 1)
	definition._add("x", "u8")._add("x", "u8")
	check("duplicate field rejected", not definition.last_error.is_empty())

	var b := Buf.new()
	b.bytes.append_array(PackedByteArray([79, 88, 83, 68]))
	b.u8(3)
	b.u8(0)
	b.u16(1)
	b.s16("Player")
	b.u32(1)
	b.u32(0)
	b.u16(2)
	b.s16("x")
	b.u8(6)
	b.u8(1)  # optional
	b.u8(0)
	b.s16("tags")
	b.u8(12)
	b.u8(0)
	b.u8(3)  # u32 elements
	b.s16("State")
	b.u32(1)
	b.u32(0)
	b.u16(1)
	b.s16("round")
	b.u8(2)
	b.u8(0)
	b.u8(0)
	b.u16(1)
	b.s16("players")
	b.u16(0)
	var parsed := CYGSchemaDef.parse_descriptor(b.bytes)
	var collections: Array = parsed.get("collections", [])
	check("v3 descriptor parses", parsed.get("version", 0) == 3 and collections.size() == 1)
	if collections.size() == 1:
		var def: CYGSchemaDef = collections[0]["schema"]
		check("v3 optional flag", bool(def.fields[0]["optional"]))
		check("v3 element type", def.fields[1]["element"] == "u32")
	check("truncated descriptor rejected",
		CYGSchemaDef.parse_descriptor(PackedByteArray([79, 88, 83, 68, 9])).is_empty())


func test_codec_golden() -> void:
	var input := CYGCodec.encode_input_frame({
		"seq": 7, "flags": 0x12345678,
		"aimYaw": 0.5, "aimPitch": -0.25, "moveX": 1.5, "moveZ": -2.0,
	})
	check("input frame golden",
		input.hex_encode() == "010700785634120000003f000080be0000c03f000000c0")
	var decoded := CYGCodec.decode_mesh_frame(input)
	check("input decode", int(decoded.get("seq", 0)) == 7 and int(decoded.get("flags", 0)) == 0x12345678)

	var state := CYGCodec.encode_state_frame(9, [{
		"id": 3, "x": 1.5, "y": -2.25, "z": 0.5, "yaw": 0.25, "pitch": -0.5,
		"vx": 0.0, "vy": 1.0, "vz": -1.0, "state": 4,
	}])
	check("state frame golden",
		state.hex_encode() == "0209000103001800dcff0800300aa1eb00001000f0ff04")

	var event := CYGCodec.encode_event_frame({
		"seq": 5, "kind": 2, "from": 3, "target": 7, "tick": 9, "value": 42.5,
	})
	check("event frame golden", event.hex_encode() == "0305000203000700090000002a42")
	var decoded_event := CYGCodec.decode_mesh_frame(event)
	check("event decode", int(decoded_event.get("target", 0)) == 7
		and absf(float(decoded_event.get("value", 0.0)) - 42.5) < 0.001)

	var delta := CYGCodec.encode_delta_frame(258, 16909060, 0, 1,
		PackedByteArray([1, 2, 3]))
	check("delta frame golden", delta.hex_encode() == "040201040302010001010203")
	check("ack golden", CYGCodec.encode_ack(258, 1).hex_encode() == "06020101")

	var signal_bytes := CYGCodec.encode_signal_frame(
		CYGCodec.SignalKind.CONFIG, 0x01020304, {"self": 5})
	check("signal frame golden",
		signal_bytes.hex_encode() == "57010800000000010203047b2273656c66223a357d")
	var signal_frame := CYGCodec.decode_signal_frame(signal_bytes)
	check("signal decode", int(signal_frame.get("to", 0)) == 0x01020304
		and int((signal_frame.get("body", {}) as Dictionary).get("self", 0)) == 5)

	var app := CYGCodec.encode_app_message("chat", "hi")
	check("app message encoding", app.hex_encode() == "0504636861746869")
	check("newer seq wraps", CYGCodec.is_newer_seq(65535, 1)
		and not CYGCodec.is_newer_seq(10, 9))


func test_sequencer() -> void:
	var seq := CYGCodec.DeltaSequencer.new()
	var first := seq.push({
		"seq": 10, "tick": 100, "fragIdx": 1, "fragTotal": 2,
		"chunk": PackedByteArray([3]),
	})
	check("fragment waits for the rest", first.is_empty())
	var assembled := seq.push({
		"seq": 10, "tick": 100, "fragIdx": 0, "fragTotal": 2,
		"chunk": PackedByteArray([1, 2]),
	})
	check("fragments reassemble",
		(assembled.get("body", PackedByteArray()) as PackedByteArray).hex_encode() == "010203")
	var stale := seq.push({
		"seq": 9, "tick": 99, "fragIdx": 0, "fragTotal": 1,
		"chunk": PackedByteArray([9]),
	})
	check("stale delta dropped", stale.is_empty() and seq.stale_count == 1)
	var newer := seq.push({
		"seq": 11, "tick": 101, "fragIdx": 0, "fragTotal": 1,
		"chunk": PackedByteArray([7]),
	})
	check("single-fragment delta applies", int(newer.get("seq", 0)) == 11 and seq.newest == 11)


func test_room_schema_and_delta() -> void:
	var room := CYGRoom.new("ws://127.0.0.1:1", "arena", {})
	room.use_descriptor(descriptor_bytes())
	check("descriptor attached", room.schema != null and room.schema.hash == player_schema().hash)
	check("root collection", String(room.root_collection) == "players")

	var counters := {"adds": 0, "removes": 0, "changes": 0, "x_values": []}
	room.on_add(func(entity, id) -> void:
		counters["adds"] = int(counters["adds"]) + 1)
	room.on_remove(func(_entity, _id) -> void:
		counters["removes"] = int(counters["removes"]) + 1)
	room.on_change(func(_entity, field, _value) -> void:
		counters["changes"] = int(counters["changes"]) + 1)

	room._apply_schema(world_full(7, [[1, 1.5, 10, 100], [2, 3.5, 20, 200]]))
	check("world full applies", room._schema_ready and room.state.size() == 2)
	check("root scalar", int(room.state.get("round")) == 7)
	var p1 = room.state.get_entity(1)
	check("typed entity", p1 != null and absf(float(p1.x) - 1.5) < 1e-9 and int(p1.hp) == 10)
	check("on_add fired twice", int(counters["adds"]) == 2)

	var field_events := {"x": []}
	p1.listen("x", func(value, _previous) -> void:
		(field_events["x"] as Array).append(float(value)))
	room._apply_schema(world_full(8, [[1, 2.5, 10, 100], [2, 4.5, 20, 200]]))
	var p1_again = room.state.get_entity(1)
	check("full snapshot preserves identity", p1_again == p1)
	check("field listener fires", (field_events["x"] as Array) == [1.5, 2.5])
	check("root update counted", int(room.state.get("round")) == 8)

	room._apply_schema(world_patch(9, patch_section(1, 7.5, 2)))
	check("patch updates field", absf(float(p1.x) - 7.5) < 1e-9)
	check("patch removes entity", room.state.get_entity(2) == null and int(counters["removes"]) == 1)
	check("patch root scalar", int(room.state.get("round")) == 9)

	var delta_room := CYGRoom.new("ws://127.0.0.1:1", "arena", {})
	delta_room.use_descriptor(descriptor_bytes())
	var payload := delta_payload(42, 77, 9, world_full(1, [[5, 3.5, 30, 300]]))
	delta_room._process_delta_payload(payload)
	check("delta session id", int(delta_room.session_id) == 42 and int(delta_room.player_id) == 42)
	check("delta ack/tick", delta_room.acked_seq == 77 and delta_room.server_tick == 9)
	check("delta entity applied", delta_room.state.get_entity(5) != null
		and absf(float(delta_room.state.get_entity(5).x) - 3.5) < 1e-9)

	var older := delta_payload(42, 78, 9, world_full(2, [[5, 9.9, 30, 300]]))
	delta_room._process_delta_payload(older)
	check("older tick ignored", absf(float(delta_room.state.get_entity(5).x) - 3.5) < 1e-9)

	var wire := CYGCodec.encode_delta_frame(3, 10, 0, 1,
		delta_payload(42, 79, 10, world_full(3, [[5, 4.5, 30, 300]])))
	delta_room._handle_delta_frame(wire)
	check("delta frame path", delta_room.delta_seq == 3 and delta_room.delta_tick == 10
		and absf(float(delta_room.state.get_entity(5).x) - 4.5) < 1e-9)

	delta_room.buffer_state(5, 10, {"x": 1.0, "y": 2.0})
	delta_room.buffer_state(5, 12, {"x": 3.0, "y": 4.0})
	var pose: Variant = delta_room.sample_state(5, delta_room.tick_to_time_ms(11.0))
	check("pose interpolation", pose != null and absf(float(pose["x"]) - 2.0) < 1e-6)
	delta_room.forget_entity(5)
	check("forget entity", delta_room.sample_state(5, 0.0) == null)

	var app_events: Array = []
	delta_room.app_message_received.connect(func(type, payload_bytes) -> void:
		app_events.append([type, payload_bytes.get_string_from_utf8()]))
	delta_room._handle_inbound_binary(CYGCodec.encode_app_message("chat", "hello"))
	check("app message inbound",
		app_events.size() == 1 and app_events[0] == ["chat", "hello"])

	var events: Array = []
	delta_room.event_received.connect(func(event) -> void:
		events.append(event))
	delta_room._handle_inbound_binary(CYGCodec.encode_event_frame({
		"seq": 4, "kind": 1, "from": 5, "target": 7, "tick": 10, "value": 1.5,
	}))
	check("event inbound", events.size() == 1 and int(events[0].get("kind", 0)) == 1)

	var voice := delta_room.attach_voice()
	delta_room._handle_inbound_binary(CYGCodec.encode_signal_frame(
		CYGCodec.SignalKind.CONFIG, 0, {"self": 5, "peers": [7]}))
	check("voice config self id", float(voice.self_id) == 5.0)
	check("voice peer roster", voice.peer_ids().has(7.0) and not voice.is_p2p(7.0))
	check("voice relay-only enable", not voice.enable())
	voice.dispose()

	delta_room._pending_pings[5] = delta_room._now_ms() - 4.0
	delta_room._handle_inbound_binary(PackedByteArray([18, 5, 0, 0, 0]))
	check("ping echo", delta_room.rtt >= 0 and not delta_room._pending_pings.has(5))


func test_callbacks_and_multi_world() -> void:
	# A patch that races the full snapshot is ignored, as in the JS client.
	var raced := CYGRoom.new("ws://127.0.0.1:1", "arena", {})
	raced.use_descriptor(descriptor_bytes())
	raced._apply_schema(patch_section(7, 1.0, 9))
	check("patch before full ignored", raced.state.size() == 0 and not raced._schema_ready)

	# Mismatched schemas surface an error instead of applying garbage.
	var mismatch := CYGRoom.new("ws://127.0.0.1:1", "arena", {})
	mismatch.use_descriptor(descriptor_bytes())
	var errors: Array = []
	mismatch.schema_error_raised.connect(func(message: String) -> void: errors.append(message))
	var bad_section := full_section([[1, 1.0, 0, 0]])
	bad_section.encode_u32(5, 0xDEADBEEF)
	mismatch._apply_schema(bad_section)
	check("schema hash mismatch reported", errors.size() == 1
		and String(errors[0]).contains("schema hash mismatch") and mismatch.state.size() == 0)

	# Colyseus immediate defaults: onAdd/listen/bindTo fire now unless deferred.
	var cb_room := CYGRoom.new("ws://127.0.0.1:1", "arena", {})
	cb_room.use_descriptor(descriptor_bytes())
	cb_room._apply_schema(world_full(0, [[1, 10.0, 20, 0]]))
	var cb := CYGCallbacks.get_callbacks(cb_room)
	var immediate_adds: Array = []
	var unsubscribe: Callable = cb.on_add("players",
		func(_entity, id) -> void: immediate_adds.append(id))
	check("on_add immediate default", immediate_adds == [1])
	unsubscribe.call()
	var deferred_adds: Array = []
	cb.on_add("players", func(_entity, id) -> void: deferred_adds.append(id), false)
	check("on_add deferred", deferred_adds.is_empty())

	var entity: CYGSchemaEntity = cb_room.state.get_entity(1)
	var seen: Array = []
	cb.listen(entity, "x", func(current, previous) -> void: seen.append([current, previous]))
	check("listen immediate default", seen == [[10.0, null]])
	var view := {}
	cb.bind_to(entity, view)
	check("bind_to immediate", is_equal_approx(float(view.get("x", -1.0)), 10.0))
	var deferred_view := {}
	cb.bind_to(entity, deferred_view, ["x"], false)
	check("bind_to deferred", not deferred_view.has("x"))
	cb_room._apply_schema(world_patch(1, update_x_section(1, 12.0)))
	check("bind_to follows", is_equal_approx(float(view["x"]), 12.0)
		and is_equal_approx(float(deferred_view["x"]), 12.0))

	# Multi-collection world: root scalars plus independent maps, callbacks
	# scoped to the collection they were registered on (JS world_schema test).
	var world := CYGRoom.new("ws://127.0.0.1:1", "world", {})
	world.use_descriptor(multi_descriptor())
	check("multi descriptor root collection", String(world.root_collection) == "players")
	check("multi descriptor aliases", world.state.bullets != null
		and world.state.players == world.state and world.state.bullets.size() == 0)
	check("multi descriptor root defaults", int(world.state.get("round")) == 0)

	var wcb := CYGCallbacks.get_callbacks(world)
	var player_adds: Array = []
	var bullet_adds: Array = []
	var player_changes: Array = []
	var round_changes: Array = []
	wcb.on_add("players", func(_entity, id) -> void: player_adds.append(id), false)
	wcb.on_add("bullets", func(_entity, id) -> void: bullet_adds.append(id), false)
	wcb.on_change("players", func(id, _entity) -> void: player_changes.append(id))
	wcb.listen(world.state, "round",
		func(current, previous) -> void: round_changes.append([current, previous]), false)

	world._apply_schema(multi_world_full(10, [[1, 10.0, 4, 0]], [[7, 2.5, 1]]))
	check("multi world full", world.state.size() == 1
		and world.state.bullets.size() == 1 and world._schema_ready)
	check("multi world values", absf(float(world.state.get_entity(1).x) - 10.0) < 1e-9
		and absf(float(world.state.bullets.get_entity(7).x) - 2.5) < 1e-9
		and int(world.state.get("round")) == 10)
	check("multi world add scoped", player_adds == [1] and bullet_adds == [7])
	check("multi world maps ready", world.state.is_ready() and world.state.bullets.is_ready())

	var immediate_bullets: Array = []
	wcb.on_add("bullets", func(_entity, id) -> void: immediate_bullets.append(id))
	check("multi immediate scoped", immediate_bullets == [7])

	world._apply_schema(multi_world_patch(11, 0, update_x_section(1, 12.0)))
	check("multi patch root", int(world.state.get("round")) == 11
		and absf(float(world.state.get_entity(1).x) - 12.0) < 1e-9)
	check("multi patch root listener", round_changes == [[10, 0], [11, 10]])
	check("multi patch change scoped", player_changes == [1] and bullet_adds == [7])

	world._apply_schema(multi_world_patch(-1, 1, projectile_add_section(8, 1.0, 2)))
	check("multi patch bullets", world.state.bullets.size() == 2
		and bullet_adds == [7, 8] and player_changes == [1])


func test_timesync_and_interpolation() -> void:
	var sync := CYGTimeSync.new()
	sync.set_tick_rate(128.0)
	for i in range(6):
		sync.add_sample(1000.0 + float(i) * 15.625, float(i * 2))
	check("timesync synced", sync.synced and sync.sample_count() == 6)
	check("timesync slope", absf(sync.ms_per_tick - 7.8125) < 1e-6)
	check("timesync roundtrip", absf(sync.time_to_tick(sync.tick_to_time_ms(50.0)) - 50.0) < 1e-6)

	var buffer := CYGInterpolationBuffer.new(func(a, b, alpha):
		return {"x": float(a["x"]) + (float(b["x"]) - float(a["x"])) * alpha}, 100.0)
	buffer.push({"x": 0.0}, 0.0)
	buffer.push({"x": 100.0}, 100.0)
	var sampled: Variant = buffer.sample(150.0)
	check("interpolation buffer blends", sampled != null and absf(float(sampled["x"]) - 50.0) < 1e-6)

	var timeline := CYGInterpolationTimeline.new({
		"interpTicks": 1, "tickDurationMs": 10.0, "maxTicksPerStep": 100,
	})
	timeline.push_snapshot(0, {7: [0.0, 0.0]}, 0.0)
	timeline.push_snapshot(1, {7: [10.0, 0.0]}, 10.0)
	timeline.push_snapshot(2, {7: [20.0, 0.0]}, 20.0)
	var blended: Variant = timeline.sample(25.0)
	var stats := timeline.stats()
	check("timeline returns entities", blended is Dictionary and (blended as Dictionary).has(7))
	check("timeline stats", int(stats.get("buffered", 0)) > 0 and float(stats.get("behind", -1.0)) >= 0.0)


func test_rollback_and_voice() -> void:
	var rollback := CYGRollbackPredictor.new(16)
	rollback.current_state = {"x": 0.0}
	var step := func(state, input):
		return {"x": float(state["x"]) + float(input)}
	var predicted := rollback.predict(1.0, step)
	check("rollback predicts", int(predicted["tick"]) == 1
		and absf(float((predicted["state"] as Dictionary)["x"]) - 1.0) < 1e-9)
	var reconciled := rollback.on_server_ack(0, {"x": 0.0}, step)
	check("rollback replays pending", bool(reconciled["rolledBack"])
		and int(reconciled["replayedTicks"]) == 1
		and absf(float((reconciled["state"] as Dictionary)["x"]) - 1.0) < 1e-9)

	check("keycode helper", CYGKeyCode.is_letter(65)
		and CYGKeyCode.is_digit(96) and CYGKeyCode.is_arrow(37)
		and CYGKeyCode.is_function_key(112) and CYGKeyCode.is_modifier(160)
		and CYGKeyCode.is_mouse_button(3))


func test_client_helpers() -> void:
	var client := CYGRelayClient.new("ws://example.com:8080/api/", {"token": "first"})
	check("http base", client.http_base() == "http://example.com:8080/api")
	check("ws base honours game port", client.ws_base({"gamePort": 7777}) == "ws://example.com:7777/api")
	check("ws base defaults", client.ws_base({}) == "ws://example.com:8080/api")

	var secure := CYGRelayClient.new("https://example.com/")
	check("secure defaults", secure.ws_base({}) == "wss://example.com:443")
	secure.free()

	client.on_auth(func() -> Variant:
		return {"token": "abc"})
	var auth := client._auth_body({"room": "x"})
	check("auth body", String((auth["body"] as Dictionary)["token"]) == "abc"
		and String(auth["token"]) == "abc")
	check("client version", CYGRelayClient.VERSION == "1.5.0")
	client.free()


func test_api_surface() -> void:
	var api := CYGApi.new("wss://example.com:8443/api/", "secret")
	check("api http base", api.http_base() == "https://example.com:8443/api")
	check("api ws base", api.ws_base() == "wss://example.com:8443/api")
	check("api build url", api.build_url("/v2/storage/save", {
		"limit": 10, "ids": ["a", "b"], "skip": null, "live": true,
	}) == "https://example.com:8443/api/v2/storage/save?ids=a&ids=b&limit=10&live=true")
	check("api route url", api.build_url("/v2/leaderboard/wins/owner/u1", {"limit": 3})
		== "https://example.com:8443/api/v2/leaderboard/wins/owner/u1?limit=3")
	check("api encodes segments", api._segment("a/b c") == "a%2Fb%20c")
	check("api signal url", api.signal_url("s 1") == "wss://example.com:8443/api/ws/signal?seat=s%201")
	check("api signal url bare", api.signal_url() == "wss://example.com:8443/api/ws/signal")
	check("api error detail", api._error_detail({
		"error": { "code": 7, "message": "Invalid request body" },
	}, "") == "Invalid request body")
	check("api error fallback", api._error_detail(null, "plain failure") == "plain failure")
	check("api version", CYGApi.VERSION == "1.5.0")
	api.free()

	var client := CYGRelayClient.new("http://127.0.0.1:8080", {"token": "first"})
	var bound := client.api()
	check("client api parent", bound.get_parent() == client and bound.http_base() == "http://127.0.0.1:8080")
	check("client api token", String(bound.resolve_token()) == "first")
	bound.set_token("second")
	check("client api token propagates", String(client.resolve_token()) == "second")
	client.on_auth(func() -> Variant:
		return {"token": "hooked"})
	check("client api hook wins", String(bound.resolve_token()) == "hooked")
	check("client request shortcut", client.has_method("request"))
	client.free()

	var routes := [
		"authenticate", "authenticate_apple", "authenticate_custom", "authenticate_device",
		"authenticate_email", "authenticate_facebook", "authenticate_facebook_instant_game",
		"authenticate_game_center", "authenticate_google", "authenticate_steam",
		"refresh_session", "logout",
		"link", "link_apple", "link_custom", "link_device", "link_email", "link_facebook",
		"link_facebook_instant_game", "link_game_center", "link_google", "link_steam",
		"unlink", "unlink_apple", "unlink_custom", "unlink_device", "unlink_email",
		"unlink_facebook", "unlink_facebook_instant_game", "unlink_game_center",
		"unlink_google", "unlink_steam",
		"get_account", "update_account", "delete_account", "get_wallet", "update_wallet",
		"merge_wallet",
		"get_users", "get_users_by_body", "get_user",
		"list_friends", "add_friends", "delete_friends", "block_friends",
		"list_friends_of_friends", "import_facebook_friends", "import_steam_friends",
		"list_groups", "create_group", "get_group", "update_group", "delete_group",
		"join_group", "leave_group", "list_group_users", "add_group_users",
		"kick_group_users", "ban_group_users", "promote_group_users", "demote_group_users",
		"list_user_groups",
		"list_tournaments", "list_tournament_records", "write_tournament_record",
		"replace_tournament_record", "delete_tournament_record",
		"list_tournament_records_around_owner", "join_tournament",
		"write_leaderboard_record", "list_leaderboard_records", "delete_leaderboard_record",
		"list_leaderboard_records_around_owner",
		"write_storage", "read_storage", "delete_storage", "list_storage_collection",
		"list_user_storage", "read_storage_object",
		"create_storage_index", "list_storage_indexes", "delete_storage_index",
		"query_storage_index",
		"list_inventory", "equip_item", "consume_item", "grant_item", "define_item",
		"list_channel_messages", "send_channel_message", "update_channel_message",
		"delete_channel_message",
		"list_notifications", "delete_notifications", "delete_notification",
		"validate_apple_purchase", "purchase_apple", "purchase_google", "purchase_huawei",
		"purchase_facebook_instant", "purchase_samsung", "list_purchases",
		"subscribe_apple", "subscribe_google", "subscribe_huawei", "list_subscriptions",
		"get_subscription",
		"list_matches", "matchmaker_stats", "matchmake", "matchmake_api",
		"status", "rooms", "health", "auto_health", "signal_url", "request",
	]
	var probe := CYGApi.new()
	var missing := []
	for route in routes:
		if not probe.has_method(route):
			missing.append(route)
	check("api route coverage (%d)" % routes.size(), missing.is_empty())
	if not missing.is_empty():
		print("  missing routes: %s" % ", ".join(missing))
	probe.free()


func test_replay_and_inspector() -> void:
	var replay := CYGRelayClient.load_replay({
		"durationMs": 1000,
		"frames": [{"t": 0, "d": [1, 2]}, {"t": 500, "d": [3, 4]}, {"t": 900, "d": [5]}],
	})
	var seen: Array = []
	replay.on_delta_callback = func(bytes, index, _total) -> void:
		seen.append([bytes, index])
	replay.seek(0.5)
	check("replay seek", seen.size() == 1 and int(seen[0][1]) == 1
		and (seen[0][0] as PackedByteArray).hex_encode() == "0304")
	replay.set_speed(99.0)
	check("replay speed clamp", replay.speed == 5.0)

	var room := CYGRoom.new("ws://127.0.0.1:1", "arena", {})
	room.use_descriptor(descriptor_bytes())
	room._apply_schema(world_full(3, [[1, 1.5, 10, 100]]))
	var inspector := CYGInspector.new(room)
	var text := inspector.render()
	var parsed: Variant = JSON.parse_string(text)
	check("inspector snapshot", parsed is Dictionary
		and int((parsed as Dictionary)["schema"]["version"]) == 1
		and ((parsed as Dictionary)["entities"] as Array).size() == 1)

	var secure_room := CYGRoom.new(" https://example.com:7777/ ", "arena", {})
	check("room endpoint normalised", secure_room.endpoint == "wss://example.com:7777"
		and secure_room.room_name == "arena")
