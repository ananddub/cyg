class_name CYGBitReader
## Zero-copy reader over a `PackedByteArray`, mirroring `CYGBitPacker`'s format.
##
## Every read advances the cursor; `remaining` and `bytes_read` let a handler
## walk a shared stream (a view delta feeds one reader to every observed
## component in turn, exactly like the Unity SDK's `ApplyDeltaStream`).

var _bytes := PackedByteArray()
var _pos := 0
## Set by `_require` when a read ran past the end of the buffer. Callers that
## walk a stream of unknown shape (an RPC argument list) check it to stop before
## appending the filler value an underflowed read returned.
var last_read_failed := false

var remaining: int:
	get:
		return _bytes.size() - _pos

var bytes_read: int:
	get:
		return _pos


func attach(data: PackedByteArray, offset: int = 0) -> void:
	_bytes = data.slice(offset) if offset > 0 else data
	_pos = 0
	last_read_failed = false


func read_byte() -> int:
	if not _require(1):
		return 0
	var value := _bytes[_pos]
	_pos += 1
	return value


func read_bool() -> bool:
	return read_byte() != 0


func read_uint32() -> int:
	var result := 0
	var shift := 0
	while shift <= 28:
		var current := read_byte()
		result |= (current & 0x7F) << shift
		if current < 0x80:
			return result
		shift += 7
	push_error("CYGBitReader: malformed uint32 varint")
	return result


func read_int32() -> int:
	var raw := read_uint32()
	return int(raw >> 1) ^ -(int(raw & 1))


func read_uint64() -> int:
	var result := 0
	var shift := 0
	while shift <= 63:
		var current := read_byte()
		result |= (current & 0x7F) << shift
		if current < 0x80:
			return result
		shift += 7
	push_error("CYGBitReader: malformed uint64 varint")
	return result


func read_int64() -> int:
	var raw := read_uint64()
	return (raw >> 1) ^ -(raw & 1)


## Reads `count` bytes, most significant byte first.
func _read_be(count: int) -> int:
	if not _require(count):
		return 0
	var value := 0
	for i in range(count):
		value = (value << 8) | int(_bytes[_pos + i])
	_pos += count
	return value


func read_uint16_be() -> int:
	return _read_be(2)


func read_int16_be() -> int:
	var raw := _read_be(2)
	return raw - 0x10000 if raw >= 0x8000 else raw


func read_uint32_be() -> int:
	return _read_be(4)


func read_single() -> float:
	return CYGBitPacker.bits_to_float(_read_be(4))


## Half-precision (IEEE 754 binary16), big-endian.
func read_half() -> float:
	return CYGBitPacker.half_to_float(_read_be(2))


func read_vector3() -> Vector3:
	return Vector3(read_single(), read_single(), read_single())


func read_vector2() -> Vector2:
	return Vector2(read_single(), read_single())


func read_vector3_packed() -> Vector3:
	return Vector3(
		float(read_int16_be()) / 100.0,
		float(read_int16_be()) / 100.0,
		float(read_int16_be()) / 100.0
	)


func read_quaternion_raw() -> Quaternion:
	return Quaternion(read_single(), read_single(), read_single(), read_single())


## Reads the 4-byte smallest-three encoding written by `write_quaternion`.
func read_quaternion() -> Quaternion:
	var packed := _read_be(4)
	var largest := (packed >> 30) & 3
	var a := _dequantize_unit((packed >> 20) & 0x3FF)
	var b := _dequantize_unit((packed >> 10) & 0x3FF)
	var c := _dequantize_unit(packed & 0x3FF)
	var sum := a * a + b * b + c * c
	var remainder := sqrt(maxf(0.0, 1.0 - sum))
	match largest:
		0:
			return Quaternion(remainder, a, b, c)
		1:
			return Quaternion(a, remainder, b, c)
		2:
			return Quaternion(a, b, remainder, c)
		_:
			return Quaternion(a, b, c, remainder)


func _dequantize_unit(encoded: int) -> float:
	return float(encoded - 512) / 512.0


func read_string() -> String:
	var count := read_uint32()
	if count == 0:
		return ""
	return _as_text(_read_exact(count))


func read_bytes(count: int) -> PackedByteArray:
	return _read_exact(count)


func read_raw(advance: int) -> void:
	_read_exact(advance)


func _read_exact(count: int) -> PackedByteArray:
	if count <= 0:
		return PackedByteArray()
	if not _require(count):
		return PackedByteArray()
	var value := _bytes.slice(_pos, _pos + count)
	_pos += count
	return value


func _as_text(data: PackedByteArray) -> String:
	return data.get_string_from_utf8()


func _require(count: int) -> bool:
	if _pos + count <= _bytes.size():
		last_read_failed = false
		return true
	push_error("CYGBitReader underflow: needed %d bytes, %d available" % [count, remaining])
	_pos = _bytes.size()
	last_read_failed = true
	return false
