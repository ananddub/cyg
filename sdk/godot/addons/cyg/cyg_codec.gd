class_name CYGCodec
extends RefCounted
## Wire codecs of the CYG client protocol, ported byte-for-byte from
## `sdk/web/cyg.js`.
##
## Three families live here:
##  * mesh frames (`0x01` input / `0x02` state / `0x03` event / `0x56` voice),
##    little-endian and quantized exactly like the Rust game server;
##  * the stamped server delta channel (`0x04` delta / `0x06` ack) plus its
##    newest-wins fragment reassembler;
##  * WebRTC signaling (`'W'` frames) and typed app messages (`0x05`).
##
## Everything is static or an inner class, so the port never needs an instance
## just to frame a packet.

const VOICE_MAGIC := 0x56
const SIGNAL_MAGIC := 0x57
const SIGNAL_VERSION := 1
const SIGNAL_HEADER_LEN := 11
const SIGNAL_BROADCAST := 0xFFFFFFFF

enum SignalKind {
	JOIN = 1, OFFER = 2, ANSWER = 3, ICE = 4, BYE = 5,
	CONFIG = 8, PEER_JOINED = 9, PEER_LEFT = 10, SIGNAL = 11,
	ERROR = 12, MUTED = 13,
}

enum MeshOp { INPUT = 0x01, STATE = 0x02, EVENT = 0x03, VOICE = 0x56 }
enum MeshEventKind { FIRE = 1, HIT = 2, DAMAGE = 3, KILL = 4, RESPAWN = 5, EFFECT = 6 }
enum AppOp { MESSAGE = 0x05 }

const APP_MESSAGE_MAX_TYPE := 255
const MESH_INPUT_LEN := 23
const MESH_EVENT_LEN := 14
const MESH_STATE_HEADER_LEN := 4
const MESH_RECORD_LEN := 19
const MESH_MAX_FRAME := 1200
const MESH_MAX_ENTITIES := int((MESH_MAX_FRAME - MESH_STATE_HEADER_LEN) / MESH_RECORD_LEN)
const MESH_LEN_SCALE := 16.0
const MESH_ANGLE_SCALE := 32768.0
const MESH_TAU := PI * 2.0

enum ChannelOp { DELTA = 0x04, ACK = 0x06 }
enum AckKind { EVENT = 1 }
const DELTA_HEADER_LEN := 9
const ACK_WIRE_LEN := 4
const MAX_DELTA_CHUNK := MESH_MAX_FRAME - DELTA_HEADER_LEN
const MAX_FRAGMENTS := 255
const FRAGMENT_TTL_MS := 3000
const DELTA_TAIL_LEN := 6
const MAX_INFLIGHT_DELTAS := 8

const SCHEMA_MAGIC := [0x4F, 0x58, 0x53, 0x32]  # "OXS2"
const DESCRIPTOR_MAGIC := [0x4F, 0x58, 0x53, 0x44]  # "OXSD"
const DESCRIPTOR_VERSION := 3

const SCHEMA_TYPE_CODE := {
	"u8": 1, "u16": 2, "u32": 3, "i32": 4, "f32": 5, "f64": 6, "bool": 7,
	"i64": 8, "i8": 9, "i16": 10, "string": 11, "array": 12,
}
const SCHEMA_FIELD_SIZE := {
	"u8": 1, "u16": 2, "u32": 4, "i32": 4, "f32": 4, "f64": 8, "bool": 1,
	"i64": 8, "i8": 1, "i16": 2, "string": 0, "array": 0,
}
const SCHEMA_TYPE_NAME := [
	"", "u8", "u16", "u32", "i32", "f32", "f64", "bool", "i64", "i8", "i16",
	"string", "array",
]
const SCHEMA_MAX_FIELDS := 32
const SCHEMA_FIELD_OPTIONAL := 1
const SCHEMA_FIELD_STATIC := 2
const SCHEMA_FIELD_TRANSIENT := 4


## ------------------------------------------------------------- primitives

## FNV-1a over UTF-8 bytes, unmasked to 32 bits (JS `>>> 0`).
static func fnv1a(hash_value: int, text: String) -> int:
	var h := hash_value & 0xFFFFFFFF
	for byte in text.to_utf8_buffer():
		h = (h ^ byte) & 0xFFFFFFFF
		h = (h * 16777619) & 0xFFFFFFFF
	return h


## Rust rounds half away from zero; Godot's `round()` uses the same rule.
static func round_half_away(v: float) -> float:
	return round(v)


## Narrow to IEEE-754 binary32 the way `Math.fround` does.
static func f32(v: float) -> float:
	var packed := PackedFloat32Array()
	packed.append(v)
	return packed[0]


## `Math.imul`-era semantics over a wrapping u16 sequence.
static func is_newer_seq(last: int, value: int) -> bool:
	var delta := (value - last) & 0xFFFF
	return delta != 0 and delta < 0x8000


static func quantize_len(v: float) -> int:
	var scaled := round_half_away(f32(v) * MESH_LEN_SCALE)
	if is_nan(scaled):
		return 0
	if scaled >= 32767.0:
		return 32767
	if scaled <= -32768.0:
		return -32768
	return int(scaled)


static func quantize_angle(v: float) -> int:
	var a := fmod(f32(v), MESH_TAU)
	if a > PI:
		a -= MESH_TAU
	elif a < -PI:
		a += MESH_TAU
	var scaled := round_half_away(a / MESH_TAU * 2.0 * MESH_ANGLE_SCALE)
	if is_nan(scaled):
		return 0
	if scaled >= 32767.0:
		return 32767
	if scaled <= -32768.0:
		return -32768
	return int(scaled)


static func dequantize_len(raw: int) -> float:
	return float(raw) / MESH_LEN_SCALE


static func dequantize_angle(raw: int) -> float:
	return float(raw) * (MESH_TAU / (2.0 * MESH_ANGLE_SCALE))


## -------------------------------------------------------------- big endian

static func read_u16_be(data: PackedByteArray, at: int) -> int:
	if at < 0 or at + 1 >= data.size():
		return 0
	return (int(data[at]) << 8) | int(data[at + 1])


static func read_u32_be(data: PackedByteArray, at: int) -> int:
	if at < 0 or at + 3 >= data.size():
		return 0
	return (int(data[at]) << 24) | (int(data[at + 1]) << 16) \
		| (int(data[at + 2]) << 8) | int(data[at + 3])


static func append_u16_be(out: PackedByteArray, value: int) -> void:
	out.append((value >> 8) & 0xFF)
	out.append(value & 0xFF)


static func append_u32_be(out: PackedByteArray, value: int) -> void:
	out.append((value >> 24) & 0xFF)
	out.append((value >> 16) & 0xFF)
	out.append((value >> 8) & 0xFF)
	out.append(value & 0xFF)


## ------------------------------------------------------------ mesh frames

## [0x01][seq u16][flags u32][aimYaw f32][aimPitch f32][moveX f32][moveZ f32]
static func encode_input_frame(input: Dictionary) -> PackedByteArray:
	var out := PackedByteArray()
	out.resize(MESH_INPUT_LEN)
	out.fill(0)
	out.encode_u8(0, MeshOp.INPUT)
	out.encode_u16(1, int(input.get("seq", 0)) & 0xFFFF)
	out.encode_u32(3, int(input.get("flags", 0)) & 0xFFFFFFFF)
	out.encode_float(7, float(input.get("aimYaw", 0.0)))
	out.encode_float(11, float(input.get("aimPitch", 0.0)))
	out.encode_float(15, float(input.get("moveX", 0.0)))
	out.encode_float(19, float(input.get("moveZ", 0.0)))
	return out


## At most `MESH_MAX_ENTITIES` 19-byte records; empty entities yield an empty
## frame, matching the JS `null` return.
static func encode_state_frame(tick: int, entities: Array) -> PackedByteArray:
	var count := mini(entities.size(), MESH_MAX_ENTITIES)
	if count == 0:
		return PackedByteArray()
	var out := PackedByteArray()
	out.resize(MESH_STATE_HEADER_LEN + count * MESH_RECORD_LEN)
	out.fill(0)
	out.encode_u8(0, MeshOp.STATE)
	out.encode_u16(1, tick & 0xFFFF)
	out.encode_u8(3, count)
	var at := MESH_STATE_HEADER_LEN
	for i in range(count):
		var entity: Dictionary = entities[i]
		out.encode_u16(at, int(entity.get("id", 0)) & 0xFFFF)
		at += 2
		out.encode_s16(at, quantize_len(float(entity.get("x", 0.0))))
		at += 2
		out.encode_s16(at, quantize_len(float(entity.get("y", 0.0))))
		at += 2
		out.encode_s16(at, quantize_len(float(entity.get("z", 0.0))))
		at += 2
		out.encode_s16(at, quantize_angle(float(entity.get("yaw", 0.0))))
		at += 2
		out.encode_s16(at, quantize_angle(float(entity.get("pitch", 0.0))))
		at += 2
		out.encode_s16(at, quantize_len(float(entity.get("vx", 0.0))))
		at += 2
		out.encode_s16(at, quantize_len(float(entity.get("vy", 0.0))))
		at += 2
		out.encode_s16(at, quantize_len(float(entity.get("vz", 0.0))))
		at += 2
		out.encode_u8(at, int(entity.get("state", 0)) & 0xFF)
		at += 1
	return out


## [0x03][seq u16][kind u8][from u16][target u16][tick u16][value f32]
static func encode_event_frame(event: Dictionary) -> PackedByteArray:
	var out := PackedByteArray()
	out.resize(MESH_EVENT_LEN)
	out.fill(0)
	out.encode_u8(0, MeshOp.EVENT)
	out.encode_u16(1, int(event.get("seq", 0)) & 0xFFFF)
	out.encode_u8(3, int(event.get("kind", 0)) & 0xFF)
	out.encode_u16(4, int(event.get("from", 0)) & 0xFFFF)
	var target: Variant = event.get("target", null)
	out.encode_u16(6, 0xFFFF if target == null else (int(target) & 0xFFFF))
	out.encode_u16(8, int(event.get("tick", 0)) & 0xFFFF)
	out.encode_float(10, float(event.get("value", 0.0)))
	return out


## One datagram in, one frame out; `{}` when the bytes are not a valid frame.
static func decode_mesh_frame(bytes: PackedByteArray) -> Dictionary:
	if bytes.is_empty():
		return {}
	var op := int(bytes[0])
	if op == MeshOp.VOICE:
		return {"op": MeshOp.VOICE, "bytes": bytes} if bytes.size() >= 15 else {}
	if op == MeshOp.INPUT:
		if bytes.size() != MESH_INPUT_LEN:
			return {}
		return {
			"op": op,
			"seq": bytes.decode_u16(1),
			"flags": bytes.decode_u32(3),
			"aimYaw": bytes.decode_float(7),
			"aimPitch": bytes.decode_float(11),
			"moveX": bytes.decode_float(15),
			"moveZ": bytes.decode_float(19),
		}
	if op == MeshOp.STATE:
		if bytes.size() < MESH_STATE_HEADER_LEN:
			return {}
		var count := int(bytes[3])
		if count == 0 or bytes.size() != MESH_STATE_HEADER_LEN + count * MESH_RECORD_LEN:
			return {}
		var entities: Array = []
		var at := MESH_STATE_HEADER_LEN
		for i in range(count):
			entities.append({
				"id": bytes.decode_u16(at),
				"x": dequantize_len(bytes.decode_s16(at + 2)),
				"y": dequantize_len(bytes.decode_s16(at + 4)),
				"z": dequantize_len(bytes.decode_s16(at + 6)),
				"yaw": dequantize_angle(bytes.decode_s16(at + 8)),
				"pitch": dequantize_angle(bytes.decode_s16(at + 10)),
				"vx": dequantize_len(bytes.decode_s16(at + 12)),
				"vy": dequantize_len(bytes.decode_s16(at + 14)),
				"vz": dequantize_len(bytes.decode_s16(at + 16)),
				"state": int(bytes[at + 18]),
			})
			at += MESH_RECORD_LEN
		return {"op": op, "tick": bytes.decode_u16(1), "count": count, "entities": entities}
	if op == MeshOp.EVENT:
		if bytes.size() != MESH_EVENT_LEN:
			return {}
		return {
			"op": op,
			"seq": bytes.decode_u16(1),
			"kind": int(bytes[3]),
			"from": bytes.decode_u16(4),
			"target": bytes.decode_u16(6),
			"tick": bytes.decode_u16(8),
			"value": bytes.decode_float(10),
		}
	return {}


## ------------------------------------------------- server delta channel

## `{}` for anything that is not a well-formed delta frame.
static func parse_delta_frame(bytes: PackedByteArray) -> Dictionary:
	if bytes.size() < DELTA_HEADER_LEN or int(bytes[0]) != ChannelOp.DELTA:
		return {}
	if bytes.size() > MESH_MAX_FRAME:
		return {}
	var frag_idx := int(bytes[7])
	var frag_total := int(bytes[8])
	if frag_total == 0 or frag_idx >= frag_total:
		return {}
	return {
		"seq": bytes.decode_u16(1),
		"tick": bytes.decode_u32(3),
		"fragIdx": frag_idx,
		"fragTotal": frag_total,
		"chunk": bytes.slice(DELTA_HEADER_LEN),
	}


static func encode_delta_frame(seq: int, tick: int, frag_idx: int, frag_total: int,
		chunk: PackedByteArray) -> PackedByteArray:
	var out := PackedByteArray()
	out.resize(DELTA_HEADER_LEN + chunk.size())
	out.fill(0)
	out.encode_u8(0, ChannelOp.DELTA)
	out.encode_u16(1, seq & 0xFFFF)
	out.encode_u32(3, tick & 0xFFFFFFFF)
	out.encode_u8(7, frag_idx & 0xFF)
	out.encode_u8(8, frag_total & 0xFF)
	for i in range(chunk.size()):
		out[DELTA_HEADER_LEN + i] = chunk[i]
	return out


static func encode_ack(seq: int, kind: int) -> PackedByteArray:
	var out := PackedByteArray()
	out.resize(ACK_WIRE_LEN)
	out.fill(0)
	out.encode_u8(0, ChannelOp.ACK)
	out.encode_u16(1, seq & 0xFFFF)
	out.encode_u8(3, kind & 0xFF)
	return out


## ------------------------------------------------------------- signaling

## `['W'][version][kind][from u32 BE][to u32 BE][opaque JSON]`.
static func encode_signal_frame(kind: int, to: int, body: Variant = null) -> PackedByteArray:
	var json := body if typeof(body) == TYPE_STRING else JSON.stringify(body if body != null else {})
	var body_bytes := String(json).to_utf8_buffer()
	var out := PackedByteArray()
	out.resize(SIGNAL_HEADER_LEN + body_bytes.size())
	out.fill(0)
	out.encode_u8(0, SIGNAL_MAGIC)
	out.encode_u8(1, SIGNAL_VERSION)
	out.encode_u8(2, kind)
	# `from` stays zero: the server stamps the real sender.
	out[3] = 0
	out[4] = 0
	out[5] = 0
	out[6] = 0
	out[7] = (to >> 24) & 0xFF
	out[8] = (to >> 16) & 0xFF
	out[9] = (to >> 8) & 0xFF
	out[10] = to & 0xFF
	for i in range(body_bytes.size()):
		out[SIGNAL_HEADER_LEN + i] = body_bytes[i]
	return out


## `{ version, kind, from, to, body }`, or `{}` when this is not a signal frame.
static func decode_signal_frame(bytes: PackedByteArray) -> Dictionary:
	if bytes.size() < SIGNAL_HEADER_LEN or int(bytes[0]) != SIGNAL_MAGIC:
		return {}
	if int(bytes[1]) != SIGNAL_VERSION:
		return {}
	var body: Variant = {}
	if bytes.size() > SIGNAL_HEADER_LEN:
		var raw := bytes.slice(SIGNAL_HEADER_LEN).get_string_from_utf8()
		if not raw.is_empty():
			var parsed: Variant = JSON.parse_string(raw)
			body = parsed if parsed != null else null
	return {
		"version": int(bytes[1]),
		"kind": int(bytes[2]),
		"from": read_u32_be(bytes, 3),
		"to": read_u32_be(bytes, 7),
		"body": body,
	}


## --------------------------------------------------------- app messages

## `[0x05][type_len u8][UTF-8 type][opaque payload]`.
static func encode_app_message(type: String, data: Variant = null) -> PackedByteArray:
	var type_bytes := type.to_utf8_buffer()
	if type_bytes.is_empty() or type_bytes.size() > APP_MESSAGE_MAX_TYPE:
		push_error("[CYG] message type must be 1..255 UTF-8 bytes")
		return PackedByteArray()
	var body := PackedByteArray()
	if typeof(data) == TYPE_STRING:
		body = String(data).to_utf8_buffer()
	elif typeof(data) == TYPE_PACKED_BYTE_ARRAY:
		body = data
	elif data != null:
		body = JSON.stringify(data).to_utf8_buffer()
	var out := PackedByteArray()
	out.resize(2 + type_bytes.size() + body.size())
	out.fill(0)
	out.encode_u8(0, AppOp.MESSAGE)
	out.encode_u8(1, type_bytes.size())
	for i in range(type_bytes.size()):
		out[2 + i] = type_bytes[i]
	for i in range(body.size()):
		out[2 + type_bytes.size() + i] = body[i]
	return out


## ------------------------------------------------------------ readers

## Bounds-checked little-endian reader; `ok` turns false on any overrun so a
## caller can reject a truncated frame instead of decoding garbage.
class Reader extends RefCounted:
	var bytes: PackedByteArray
	var offset := 0
	var ok := true

	func _init(data: PackedByteArray = PackedByteArray()) -> void:
		bytes = data

	func remaining() -> int:
		return bytes.size() - offset

	func _take(count: int) -> bool:
		if offset + count > bytes.size():
			ok = false
			return false
		return true

	func read_u8() -> int:
		if not _take(1):
			return 0
		var value := int(bytes[offset])
		offset += 1
		return value

	func read_i8() -> int:
		if not _take(1):
			return 0
		var value := int(bytes[offset])
		offset += 1
		return value - 256 if value >= 128 else value

	func read_u16() -> int:
		if not _take(2):
			return 0
		var value := bytes.decode_u16(offset)
		offset += 2
		return value

	func read_s16() -> int:
		if not _take(2):
			return 0
		var value := bytes.decode_s16(offset)
		offset += 2
		return value

	func read_u32() -> int:
		if not _take(4):
			return 0
		var value := bytes.decode_u32(offset)
		offset += 4
		return value

	func read_s32() -> int:
		if not _take(4):
			return 0
		var value := bytes.decode_s32(offset)
		offset += 4
		return value

	func read_f32() -> float:
		if not _take(4):
			return 0.0
		var value := bytes.decode_float(offset)
		offset += 4
		return value

	func read_f64() -> float:
		if not _take(8):
			return 0.0
		var value := bytes.decode_double(offset)
		offset += 8
		return value

	func read_i64() -> int:
		if not _take(8):
			return 0
		var value := bytes.decode_s64(offset)
		offset += 8
		return value

	func read_bytes(count: int) -> PackedByteArray:
		if not _take(count):
			return PackedByteArray()
		var value := bytes.slice(offset, offset + count)
		offset += count
		return value

	func skip(count: int) -> void:
		if not _take(count):
			return
		offset += count

	## `u16`-prefixed UTF-8 string, the only string shape in OXSD/OXS2.
	func read_string16() -> String:
		var length := read_u16()
		return read_bytes(length).get_string_from_utf8()


## ------------------------------------------------------- delta sequencer

## Newest-wins filter plus concurrent fragment reassembler for `0x04` frames.
class DeltaSequencer extends RefCounted:
	var newest := -1
	var partials := {}  # seq -> { seq, tick, total, parts, received, at }
	var stale_count := 0
	var assembled_count := 0
	var expired_count := 0
	var now_fn: Callable

	func _init(now: Callable = Callable()) -> void:
		now_fn = now

	func _now() -> int:
		return int(now_fn.call()) if now_fn.is_valid() else Time.get_ticks_msec()

	func push(frame: Dictionary) -> Dictionary:
		var seq: int = int(frame.get("seq", 0))
		if newest != -1 and (seq == newest or not CYGCodec.is_newer_seq(newest, seq)):
			stale_count += 1
			return {}
		var now := _now()
		var total: int = int(frame.get("fragTotal", 0))
		if total == 1:
			_apply(seq)
			return {"seq": seq, "tick": int(frame.get("tick", 0)), "body": frame.get("chunk", PackedByteArray())}

		var pending: Dictionary = partials.get(seq, {})
		# A changed frag_total means the server restarted the delta.
		if not pending.is_empty() and int(pending["total"]) != total:
			partials.erase(seq)
			pending = {}
		if pending.is_empty():
			if partials.size() >= CYGCodec.MAX_INFLIGHT_DELTAS:
				_evict_stalest()
			pending = {
				"seq": seq,
				"tick": int(frame.get("tick", 0)),
				"total": total,
				"parts": {},
				"received": 0,
				"at": now,
			}
			partials[seq] = pending
		pending["at"] = now
		var frag_idx: int = int(frame.get("fragIdx", 0))
		if not pending["parts"].has(frag_idx):
			pending["parts"][frag_idx] = frame.get("chunk", PackedByteArray())
			pending["received"] = int(pending["received"]) + 1
		if int(pending["received"]) != int(pending["total"]):
			return {}
		var body := PackedByteArray()
		for i in range(int(pending["total"])):
			body.append_array(pending["parts"][i])
		var applied := {"seq": seq, "tick": int(pending["tick"]), "body": body}
		partials.erase(seq)
		_apply(seq)
		return applied

	## Makes `seq` authoritative and retires every set it supersedes.
	func _apply(seq: int) -> void:
		newest = seq
		for buffered in partials.keys():
			if not CYGCodec.is_newer_seq(seq, int(buffered)):
				partials.erase(buffered)
		assembled_count += 1

	## Drops the set that has progressed least recently.
	func _evict_stalest() -> void:
		var victim: Variant = null
		for buffered in partials.keys():
			if victim == null or int(partials[buffered]["at"]) < int(partials[victim]["at"]):
				victim = buffered
		if victim != null:
			partials.erase(victim)
			expired_count += 1

	## Forgets half-received deltas that stopped progressing past the TTL.
	func cleanup() -> bool:
		var now := _now()
		var expired := false
		for buffered in partials.keys():
			if now - int(partials[buffered]["at"]) > CYGCodec.FRAGMENT_TTL_MS:
				partials.erase(buffered)
				expired_count += 1
				expired = true
		return expired
