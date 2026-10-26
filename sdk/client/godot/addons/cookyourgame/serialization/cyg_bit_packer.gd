class_name CYGBitPacker
## Byte-aligned binary writer for CYG packets.
##
## Wire-compatible with `CYGBitWriter` in the Unity SDK: LEB128 unsigned and
## zig-zag signed varints, big-endian IEEE754 floats, big-endian fixed fields,
## int16×100 packed vectors and the 4-byte smallest-three quaternion encoding.
## SyncVar state patches and RPC argument streams are built on this writer.

var _bytes := PackedByteArray()
var _pos := 0

var length: int:
	get:
		return _pos


func reset() -> void:
	_pos = 0


func to_bytes() -> PackedByteArray:
	return _bytes.slice(0, _pos)


func _ensure(extra: int) -> void:
	if _bytes.size() < _pos + extra:
		_bytes.resize(_pos + extra)


func write_byte(value: int) -> void:
	_ensure(1)
	_bytes[_pos] = value & 0xFF
	_pos += 1


func write_bool(value: bool) -> void:
	write_byte(1 if value else 0)


func write_uint32(value: int) -> void:
	_ensure(5)
	var v := value & 0xFFFFFFFF
	while v >= 0x80:
		_bytes[_pos] = (v & 0x7F) | 0x80
		_pos += 1
		v >>= 7
	_bytes[_pos] = v
	_pos += 1


func write_int32(value: int) -> void:
	var zig := ((value << 1) ^ (value >> 31)) & 0xFFFFFFFF
	write_uint32(zig)


func write_uint64(value: int) -> void:
	_ensure(10)
	var v := value
	while (v & ~0x7F) != 0:
		_bytes[_pos] = (int(v) & 0x7F) | 0x80
		_pos += 1
		v = (v >> 7) & 0x1FFFFFFFFFFFFFF
	_bytes[_pos] = v & 0x7F
	_pos += 1


func write_int64(value: int) -> void:
	write_uint64(((value << 1) ^ (value >> 63)) & 0x7FFFFFFFFFFFFFFF)


## Writes `count` bytes of `value`, most significant byte first.
func _write_be(value: int, count: int) -> void:
	_ensure(count)
	for i in range(count):
		_bytes[_pos + i] = (value >> (8 * (count - 1 - i))) & 0xFF
	_pos += count


func write_uint16_be(value: int) -> void:
	_write_be(value & 0xFFFF, 2)


func write_uint32_be(value: int) -> void:
	_write_be(value & 0xFFFFFFFF, 4)


func write_single(value: float) -> void:
	_write_be(float_to_bits(value), 4)


## Half-precision (IEEE 754 binary16), big-endian, matching Unity's WriteHalf.
func write_half(value: float) -> void:
	_write_be(float_to_half(value), 2)


func write_vector3(value: Vector3) -> void:
	write_single(value.x)
	write_single(value.y)
	write_single(value.z)


func write_vector2(value: Vector2) -> void:
	write_single(value.x)
	write_single(value.y)


## Three int16 fixed-point components at 1 unit per 100, big-endian.
func write_vector3_packed(value: Vector3) -> void:
	_write_packed_axis(value.x)
	_write_packed_axis(value.y)
	_write_packed_axis(value.z)


func _write_packed_axis(value: float) -> void:
	var scaled := clampi(int(roundf(value * 100.0)), -32768, 32767)
	_write_be(scaled & 0xFFFF, 2)


## Four full-precision floats, big-endian.
func write_quaternion_raw(value: Quaternion) -> void:
	write_single(value.x)
	write_single(value.y)
	write_single(value.z)
	write_single(value.w)


## Smallest-three encoding in exactly 4 bytes: 2-bit index of the omitted
## (largest-magnitude) component plus three 10-bit signed components.
func write_quaternion(value: Quaternion) -> void:
	var components := [value.x, value.y, value.z, value.w]
	var largest := 0
	var max_abs := absf(components[0])
	for i in range(1, 4):
		if absf(components[i]) > max_abs:
			largest = i
			max_abs = absf(components[i])
	var packed := largest << 30
	var slot := 0
	for i in range(4):
		if i == largest:
			continue
		packed |= _quantize_unit(components[i]) << (20 - slot * 10)
		slot += 1
	_write_be(packed, 4)


func _quantize_unit(value: float) -> int:
	if is_nan(value):
		return 512
	return clampi(int(roundf(value * 512.0)) + 512, 0, 1023)


## LEB128 byte length followed by UTF-8 bytes.
func write_string(value: String) -> void:
	if value.is_empty():
		write_uint32(0)
		return
	var utf8 := value.to_utf8_buffer()
	write_uint32(utf8.size())
	write_raw(utf8)


## LEB128 length followed by raw payload bytes.
func write_bytes(data: PackedByteArray) -> void:
	write_uint32(data.size())
	write_raw(data)


## Appends bytes verbatim without a length prefix.
func write_raw(data: PackedByteArray) -> void:
	var count := data.size()
	if count == 0:
		return
	_bytes.resize(_pos)
	_bytes.append_array(data)
	_pos = _bytes.size()


static func float_to_half(value: float) -> int:
	if is_nan(value):
		return 0x7FE0
	var bits := float_to_bits(value)
	var sign := (bits >> 16) & 0x8000
	var exponent := ((bits >> 23) & 0xFF) - 127
	var mantissa := bits & 0x7FFFFF
	if exponent == 128:
		return sign | 0x7C00 | (0x200 if mantissa != 0 else 0)
	var half_exponent := exponent + 15
	if half_exponent >= 31:
		return sign | 0x7C00
	if half_exponent <= 0:
		if half_exponent < -10:
			return sign
		mantissa = mantissa | 0x800000
		var shift := 14 - half_exponent
		var truncated := mantissa >> shift
		if (mantissa & ((1 << shift) - 1)) > (1 << (shift - 1)):
			truncated += 1
		return sign | truncated
	var half_mantissa := mantissa >> 13
	var result := (half_exponent << 10) | half_mantissa
	var dropped := mantissa & 0x1FFF
	if dropped > 0x1000 or (dropped == 0x1000 and (half_mantissa & 1) != 0):
		result += 1
	return sign | result


static func half_to_float(half: int) -> float:
	var sign := (half & 0x8000) << 16
	var exponent := (half >> 10) & 0x1F
	var mantissa := half & 0x3FF
	var bits := 0
	if exponent == 0:
		if mantissa == 0:
			bits = sign
		else:
			exponent = 1
			while (mantissa & 0x400) == 0:
				mantissa <<= 1
				exponent -= 1
			mantissa &= 0x3FF
			bits = sign | ((exponent + 112) << 23) | (mantissa << 13)
	elif exponent == 31:
		bits = sign | 0x7F800000 | (mantissa << 13)
	else:
		bits = sign | ((exponent + 112) << 23) | (mantissa << 13)
	return bits_to_float(bits)


static var _bits_scratch := PackedByteArray()


static func float_to_bits(value: float) -> int:
	if _bits_scratch.size() < 4:
		_bits_scratch.resize(4)
	_bits_scratch.encode_float(0, value)
	return (int(_bits_scratch[0]) | (int(_bits_scratch[1]) << 8) | (int(_bits_scratch[2]) << 16) | (int(_bits_scratch[3]) << 24)) & 0xFFFFFFFF


static func bits_to_float(bits: int) -> float:
	if _bits_scratch.size() < 4:
		_bits_scratch.resize(4)
	_bits_scratch[0] = bits & 0xFF
	_bits_scratch[1] = (bits >> 8) & 0xFF
	_bits_scratch[2] = (bits >> 16) & 0xFF
	_bits_scratch[3] = (bits >> 24) & 0xFF
	return _bits_scratch.decode_float(0)
