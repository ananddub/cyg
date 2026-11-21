# Schema decode smoke test for the Godot addon — driven through the
# cyg-core-rs GDExtension (the GDScript OXS2 decoder was removed; ALL schema
# decode lives in the Rust core, same as the web/Unity/Unreal SDKs).
#
#   godot --headless --path sdk/client/godot -s res://test/schema_decoder_test.gd
extends SceneTree

var checks := 0
var failures := 0


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

	var _sp := StreamPeerBuffer.new()

	func f64(v: float) -> void:
		_sp.clear()
		_sp.put_double(v)
		bytes.append_array(_sp.data_array)


func check(name: String, ok: bool) -> void:
	checks += 1
	if ok:
		print("CHECK " + name)
	else:
		failures += 1
		printerr("FAIL " + name)


## [OXS2][kind=5][hash u32][count u32](rows: [id varint][hp u32][x f64])…
## The wire is bit-packed: `hp` is an exact 32-bit integer in the bit stream,
## `x` is an f64 without a declared range, so it goes in the variable payload.
func section_full(hash: int, rows: Array) -> PackedByteArray:
	var b := Buf.new()
	for m in [0x4F, 0x58, 0x53, 0x32]:  # "OXS2"
		b.u8(m)
	b.u8(5)
	b.u32(hash)
	b.u32(rows.size())
	for row in rows:
		b.u8(row[0])  # LEB128 id (ids below 128 are one byte)
		b.u32(row[2]) # hp, 32 bits LSB-first == LE bytes
		b.f64(row[1]) # x, variable payload
	return b.bytes


## [your_id i64]["OXS2"]…[acked u16][tick u32] — the envelope the core unwraps.
func envelope(your_id: int, section: PackedByteArray) -> PackedByteArray:
	var out := PackedByteArray()
	out.append(0xA5)
	for c in "delta".to_ascii_buffer():
		out.append(c)
	for i in range(8):
		out.append((your_id >> (8 * i)) & 0xFF)
	out.append_array(section)
	out.append(0)  # acked u16
	out.append(0)
	out.append(60)  # tick u32
	out.append(0)
	out.append(0)
	out.append(0)
	out.append(0)  # client_stamp u16
	out.append(0)
	return out


func _initialize() -> void:
	if not ClassDB.class_exists("CygRoom"):
		check("cyg-core GDExtension loaded", false)
		print("checks=%d failures=%d" % [checks, failures])
		quit(1)
		return
	check("cyg-core GDExtension loaded", true)

	var core: Variant = CygRoom.new()
	var schema := CYGSchemaDef.new("Player", 1).f64("x").u32("hp")
	var descriptor: PackedByteArray = CYGSchemaDef.build_descriptor_v2("players", null, [{"name": "players", "schema": schema}])
	check("descriptor built", descriptor.size() > 0)
	check("descriptor loads in the core", core.load_descriptor(descriptor))

	# The core's parsed metadata matches what was built.
	check("schema count", int(core.schema_count()) == 1)
	check("schema name", String(core.schema_name(0)) == "Player")
	check("schema version", int(core.schema_version(0)) == 1)
	check("schema field count", int(core.schema_field_count(0)) == 2)
	check("schema field names",
			String(core.schema_field_name(0, 0)) == "x" and String(core.schema_field_name(0, 1)) == "hp")
	check("schema field kinds", int(core.schema_field_kind(0, 0)) == 7 and int(core.schema_field_kind(0, 1)) == 4)
	var player_hash := int(core.schema_hash(0))
	check("schema hash non-zero", player_hash != 0)
	check("collection name", String(core.collection_name(0)) == "players")
	check("not ready before a full snapshot", not core.ready())

	# A full snapshot: two entities. Applied with the [0xA5 "delta"] prefix
	# the wire carries, so the core unwraps the whole envelope.
	var full := section_full(player_hash, [[7, 100.5, 3], [9, -4.25, 0]])
	var events := int(core.apply_payload(envelope(42, full)))
	check("full snapshot applied", events >= 0)
	check("ready after full snapshot", core.ready())
	check("entity count", int(core.entity_count()) == 2)
	check("entity ids ascending",
			int(core.entity_id_at_in(0, 0)) == 7 and int(core.entity_id_at_in(0, 1)) == 9)

	var xi := int(core.field_index("x"))
	var hi := int(core.field_index("hp"))
	check("field indexes", xi == 0 and hi == 1)
	check("f64 reads", absf(float(core.get_f64_in(0, 7, xi)) - 100.5) < 1e-9
			and absf(float(core.get_f64_in(0, 9, xi)) - (-4.25)) < 1e-9)
	check("u32 read through f64 view", float(core.get_f64_in(0, 7, hi)) == 3.0)
	check("absent entity reads NaN", is_nan(float(core.get_f64_in(0, 123, xi))))

	# Truncated/garbage frames are rejected without throwing.
	check("garbage frame rejected", int(core.apply_payload(PackedByteArray([0xA5, 1, 2, 3]))) == -1)

	# Schema-lane input encoding through the core: no Input schema declared,
	# so encoding fails cleanly (empty bytes).
	var packed: PackedByteArray = core.encode_schema_input(1, PackedInt64Array([1, 2]))
	check("no Input schema → empty encode", packed == null or packed.size() == 0)

	print("checks=%d failures=%d" % [checks, failures])
	quit(1 if failures > 0 else 0)
