# Schema decoder smoke test for the Godot addon.
#
#   godot --headless --path sdk/godot --editor --quit
#   godot --headless --path sdk/godot -s res://test/schema_decoder_test.gd
extends SceneTree

const PLAYER_HASH := 0x11111111
const ROOT_HASH := 0x22222222

var checks := 0
var failures := 0


class Player extends OpenOxideSchema.Entity:
	var x: float = 0.0
	var hp: int = 0
	var tags: Array = []
	var target = null

	func set_field(field: int, value) -> void:
		match field:
			0:
				x = float(value)
			1:
				hp = int(value)
			2:
				tags = value
			3:
				target = null if value == null else int(value)

	func get_field(field: int):
		match field:
			0:
				return x
			1:
				return hp
			2:
				return tags
			3:
				return target
		return null

	func clear_fields() -> void:
		x = 0.0
		hp = 0
		tags = []
		target = null


class Buf:
	var bytes := PackedByteArray()

	func u8(v: int) -> void:
		bytes.append(v & 0xff)

	func u16(v: int) -> void:
		u8(v & 0xff)
		u8((v >> 8) & 0xff)

	func u32(v: int) -> void:
		u16(v & 0xffff)
		u16((v >> 16) & 0xffff)

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


func check(name: String, ok: bool) -> void:
	checks += 1
	if not ok:
		failures += 1
	print(("CHECK " if ok else "FAIL  ") + name)


func add_field(b: Buf, name: String, code: int, flags: int, element: int) -> void:
	b.s16(name)
	b.u8(code)
	b.u8(flags)
	b.u8(element)


func descriptor() -> PackedByteArray:
	var b := Buf.new()
	b.bytes.append_array(PackedByteArray([79, 88, 83, 68]))
	b.u8(3)
	b.u8(0)
	b.u16(1)
	b.s16("Player")
	b.u32(1)
	b.u32(PLAYER_HASH)
	b.u16(4)
	add_field(b, "x", 6, 0, 0)
	add_field(b, "hp", 1, 0, 0)
	add_field(b, "tags", 12, 0, 3)
	add_field(b, "target", 3, 1, 0)
	b.s16("State")
	b.u32(1)
	b.u32(ROOT_HASH)
	b.u16(1)
	add_field(b, "round", 2, 0, 0)
	b.u16(1)
	b.s16("players")
	b.u16(0)
	return b.bytes


func row(s: Buf, id: int, x: float, hp: int, tags: Array, target) -> void:
	s.u32(id)
	s.f64(x)
	s.u8(hp)
	s.u8(tags.size())
	for tag in tags:
		s.u32(tag)
	if target == null:
		s.u8(0)
	else:
		s.u8(1)
		s.u32(target)


func section_full() -> PackedByteArray:
	var s := Buf.new()
	s.magic()
	s.u8(1)
	s.u32(PLAYER_HASH)
	s.u32(2)
	row(s, 1, 1.5, 10, [7, 9], 42)
	row(s, 2, 3.5, 20, [], null)
	return s.bytes


func world_collection(f: Buf, id: int, hash: int, section: PackedByteArray) -> void:
	f.u16(id)
	f.u32(hash)
	f.u32(section.size())
	f.bytes.append_array(section)


func world_full() -> PackedByteArray:
	var f := Buf.new()
	f.magic()
	f.u8(3)
	f.u32(ROOT_HASH)
	f.u16(1)
	f.u16(7)
	f.u16(1)
	world_collection(f, 0, PLAYER_HASH, section_full())
	return f.bytes


func world_patch() -> PackedByteArray:
	var f := Buf.new()
	f.magic()
	f.u8(4)
	f.u32(ROOT_HASH)
	f.u16(1)
	f.u32(1)
	f.u16(8)

	var p := Buf.new()
	p.magic()
	p.u8(2)
	p.u32(PLAYER_HASH)
	p.u32(2)
	p.u8(3)
	p.u32(1)
	p.u32((1 << 0) | (1 << 3))
	p.f64(9.5)
	p.u8(0)
	p.u8(2)
	p.u32(2)
	f.u16(1)
	world_collection(f, 0, PLAYER_HASH, p.bytes)
	return f.bytes


func _initialize() -> void:
	var registry := OpenOxideSchema.Registry.new()
	registry.register("Player", func(): return Player.new())

	var state = OpenOxideSchema.State.create_from_descriptor(descriptor(), registry)
	check("descriptor parses", state != null)
	if state == null:
		quit(1)
		return
	check("bad descriptor rejected", OpenOxideSchema.Descriptor.parse(PackedByteArray([79, 88, 83, 68, 9])) == null)

	var counters := {"adds": 0, "removes": 0, "changes": 0, "updates": 0, "root_changes": 0}
	state.entity_added.connect(func(_name, _id, _entity): counters["adds"] += 1)
	state.entity_removed.connect(func(_name, _id, _entity): counters["removes"] += 1)
	state.field_changed.connect(func(_name, _id, _field, _value, _previous): counters["changes"] += 1)
	state.entity_updated.connect(func(_name, _id, _entity): counters["updates"] += 1)
	state.root_field_changed.connect(func(_field, _value, _previous): counters["root_changes"] += 1)

	check("world full applies", state.apply(world_full()))
	check("ready", state.ready)
	check("added two", counters["adds"] == 2 and state.collection("players").order.size() == 2)

	var p1 = state.collection("players").find(1)
	var p2 = state.collection("players").find(2)
	check("typed row", p1 is Player and p1.x == 1.5 and p1.hp == 10)
	check("array field", p1.tags.size() == 2 and p1.tags[1] == 9)
	check("optional present", p1.target == 42)
	check("optional absent", p2.target == null and p2.tags.is_empty())
	check("root scalar", state.root_entity.get_field(0) == 7)

	check("world patch applies", state.apply(world_patch()))
	check("root patched", state.root_entity.get_field(0) == 8)
	check("field updated", p1.x == 9.5 and p1.target == null)
	check("entity removed", state.collection("players").find(2) == null)
	check("events", counters["removes"] == 1 and counters["changes"] == 2
		and counters["updates"] == 1 and counters["root_changes"] == 2)

	var bare = OpenOxideSchema.State.create_from_descriptor(descriptor(), registry)
	check("patch before full dropped", bare != null and not bare.apply(world_patch()))
	check("truncated rejected", not state.apply(PackedByteArray([79, 88, 83, 50, 1, 0])))
	check("foreign rejected", not state.apply(PackedByteArray([1, 2, 3, 4, 5])))

	print("checks=%d failures=%d" % [checks, failures])
	quit(1 if failures > 0 else 0)
