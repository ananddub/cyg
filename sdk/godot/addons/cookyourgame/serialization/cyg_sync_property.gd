class_name CYGSyncProperty
extends RefCounted
## One reactive sync value: game code writes it, listens to `value_changed`, and
## the wire carries only the bytes that changed.
##
## This is the scalar sibling of `CYGPropertyMap` and reuses that class's tagged
## value codec through `CYGDeltaPacker.write_value()`/`read_value()`, so a property
## can hold a `Vector3`, a `Color` or a `PackedByteArray` with no second encoder.
## Unlike the map, which addresses entries by registration index and encodes each
## with its registered kind, a `CYGSyncProperty` is standalone and therefore tags
## every value it writes: the tag is `typeof(value)`, so a receiver never needs a
## matching registration order.
##
## Delta blob, byte-exact:
##
##     [u8 present]            1 when a value follows, 0 when nothing changed
##     [u8 tag][payload]       only when present == 1, see CYGDeltaPacker
##
## The presence byte is written even when clean. `CYGView`'s per-component dirty
## mask decides *whether* this blob appears at all, but a receiver must never infer
## a field's length from a sender's flag it has not been told, so the blob is
## always self-describing and `deserialize()` consumes exactly what `serialize()`
## wrote on every peer.
##
## Initial sync for a late joiner uses the same framing as `serialize()`:
## `write_full_state()` emits `present = 1` plus the value.
##
## Usage, inside a `CYGBehaviour` component:
##
##     var _health := CYGSyncProperty.new("health", 100)
##
##     func _ready() -> void:
##         _health.value_changed.connect(_on_health_changed)
##         register_as_observable()
##
##     func _on_health_changed(prop: String, old_value: Variant, new_value: Variant) -> void:
##         label.text = "%d" % new_value
##
##     # The component owns the authority gate and the forwarding hooks the view
##     # walks:
##     func has_delta() -> bool:
##         return _health.has_delta() or super.has_delta()
##
##     func serialize(writer: CYGBitPacker) -> void:
##         super.serialize(writer)
##         _health.serialize(writer)
##
##     func deserialize(reader: CYGBitReader) -> void:
##         super.deserialize(reader)
##         _health.deserialize(reader)
##
##     # A joiner's snapshot blob:
##     func write_full_state(writer: CYGBitPacker) -> void:
##         _health.write_full_state(writer)
##
## Writes on a proxy are local-only predictions; gate them with
## `CYGBehaviour.has_authority` (or `set_property`-style checks) if the value is
## authoritative.
##
## Design derives from Netfox's MIT-licensed reactive properties and Mirror's
## SyncVar (attribution in LICENSE-NETFOX.md). Clean-room GDScript; no upstream
## source text is copied.

signal value_changed(property_name: String, old_value: Variant, new_value: Variant)

var _name: String = ""
var _value: Variant = null
# Pinned at construction from the initial value's type. TYPE_NIL means "unpinned":
# a property that started as null accepts any encodable type and re-tags per write.
var _kind: int = TYPE_NIL
var _dirty := false


## Creates a property called `name` holding `initial`. Pass a non-null `initial`
## to pin the value's type for the whole session.
func _init(name: String = "", initial: Variant = null) -> void:
	_name = name
	_value = initial
	_kind = typeof(initial)


var property_name: String:
	get:
		return _name


## The current value. Assigning is the same as `set_value()`, minus the result.
var value: Variant:
	get:
		return _value
	set(new_value):
		set_value(new_value)


var is_dirty: bool:
	get:
		return _dirty


## The pinned value type (`typeof(initial)`), or TYPE_NIL when unpinned.
func kind() -> int:
	return _kind


func get_value() -> Variant:
	return _value


## Authority-side write. A value equal to the current one is dropped, exactly like
## `CYGPropertyMap.set_value`; a type change on a pinned property is rejected.
## `value_changed` fires for local writes *and* remote applies, so listeners see
## every visible change.
func set_value(new_value: Variant) -> bool:
	if _kind != TYPE_NIL and typeof(new_value) != _kind:
		push_error("CYGSyncProperty.set_value: '%s' is pinned to type %d, got %d." % [_name, _kind, typeof(new_value)])
		return false
	if not CYGDeltaPacker.supports(new_value):
		push_error("CYGSyncProperty.set_value: '%s' cannot hold type %d." % [_name, typeof(new_value)])
		return false
	_assign(new_value, true)
	return true


## Stores `new_value` without dirtying it and without emitting `value_changed`,
## for construction-time seeding (a scene default read from the inspector).
func reset(new_value: Variant) -> void:
	_value = new_value
	_dirty = false


## The full state a late joiner must start from.
func read_full_state() -> Variant:
	return _value


## Writes the full state in `serialize()`'s framing, ignoring the dirty flag, and
## clears it. Use for a joiner's snapshot or a spawn payload.
func write_full_state(writer: CYGBitPacker) -> void:
	writer.write_byte(1)
	CYGDeltaPacker.write_value(writer, _value)
	_dirty = false


## Seeds from a decoded full state (a spawn payload, not a delta stream). Emits
## `value_changed` when the value actually moves.
func apply_full_state(new_value: Variant) -> void:
	_assign(new_value, true)
	_dirty = false


func has_delta() -> bool:
	return _dirty


## Clears the dirty flag without sending, for the case where the component decided
## to skip the tick (a `SendInterval` throttle, or a proxy that owns the value).
func mark_clean() -> void:
	_dirty = false


## `[u8 present]` and, when dirty, `[u8 tag][payload]`. Clean writes consume the
## same bytes on every peer.
func serialize(writer: CYGBitPacker) -> void:
	if not _dirty:
		writer.write_byte(0)
		return
	writer.write_byte(1)
	CYGDeltaPacker.write_value(writer, _value)
	_dirty = false


func deserialize(reader: CYGBitReader) -> bool:
	var present := reader.read_byte()
	if reader.last_read_failed:
		return false
	if present == 0:
		return true
	if present != 1:
		# Nothing about the payload can be measured from here, so the caller must
		# resync this property rather than keep walking the stream.
		push_error("CYGSyncProperty: unknown presence byte %d on '%s'; stream is out of sync." % [present, _name])
		return false
	var new_value: Variant = CYGDeltaPacker.read_value(reader)
	if reader.last_read_failed:
		push_error("CYGSyncProperty: value for '%s' ran past the end of the stream." % _name)
		return false
	# A pinned-type violation is reported but still applied: diverging from the
	# authority is worse than holding a value of the wrong shape.
	if _kind != TYPE_NIL and typeof(new_value) != _kind:
		push_error("CYGSyncProperty: remote wrote type %d into pinned '%s' (%d)." % [typeof(new_value), _name, _kind])
	_assign(new_value, false)
	# An applied value IS this peer's new baseline, so it must not look like local
	# intent: leaving it dirty would make every proxy re-forward what it just
	# received, and the room would echo the same write forever.
	_dirty = false
	return true


func _assign(new_value: Variant, mark_dirty: bool) -> void:
	var previous: Variant = _value
	if CYGDeltaPacker.values_equal(previous, new_value):
		return
	_value = new_value
	if mark_dirty:
		_dirty = true
	value_changed.emit(_name, previous, new_value)
