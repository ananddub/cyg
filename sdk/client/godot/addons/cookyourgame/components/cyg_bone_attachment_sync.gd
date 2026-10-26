class_name CYGBoneAttachmentSync
extends CYGBehaviour
## BoneAttachment3D weapon/hat socket sync.
##
## A weapon spawned by a player attaches to a bone on that player's avatar on
## EVERY peer. Nothing about the bone transform itself crosses the wire — each
## peer's own `Skeleton3D` sampling moves the `BoneAttachment3D` at full render
## rate, so this component only syncs the small immutable facts: the bone NAME
## (the identity that survives between peers; indices are resolved locally with
## `Skeleton3D.find_bone`), the socket slot, the attached/detached state and the
## local offset the attachment node keeps under the bone. Works on avatars whose
## animation is itself networked (e.g. driven by `CYGAnimationSync`): attachment
## is to the bone, whatever pose the local mixer puts on it.
##
## Two roles, fixed per scene instance so registration order is peer-identical:
##
## ROLE_SOCKET_PROVIDER sits on the avatar as a child of a `BoneAttachment3D`
## (under the `Skeleton3D`, so it is NOT a direct view child and never receives
## `cyg_on_spawned`; it listens to the view's `spawned`/`owner_changed` signals
## instead). It publishes actor -> slot -> bone into a static registry and costs
## zero wire bytes: never observable, `has_delta()` false, no payload.
##
## ROLE_ATTACHMENT sits on the weapon's networked node. Payload after the base
## sync-property block (empty here): [byte flags] ALWAYS, then in ascending bit
## order FLAG_STATE [byte slot][bool attached], FLAG_BONE [string bone name],
## FLAG_OFFSET [vector3 position][quaternion rotation].
##
## State changes are rare (equip/unequip/re-sheathe), so everything is event-
## driven: a steady-state weapon sends nothing.

const FLAG_STATE := 1
const FLAG_BONE := 2
const FLAG_OFFSET := 4

enum Role { ATTACHMENT, SOCKET_PROVIDER }

## Which half of the socket pair this instance is.
@export var role: Role = Role.ATTACHMENT
## Bone name this socket provides / this attachment targets.
@export var bone_name: StringName = &""
## Disambiguates several attachments on one bone (two holsters, back + hip).
@export var socket_slot: int = 0
## Local offset applied under the bone on attach (attachment role).
@export var offset_position: Vector3 = Vector3()
## Local rotation applied under the bone on attach (attachment role).
@export var offset_rotation: Quaternion = Quaternion.IDENTITY

## actor -> (slot -> BoneAttachment3D); per-actor dictionaries are reused, and
## every lookup path drops invalid instances, so a despawned avatar cannot leak.
static var _providers: Dictionary = {}

var _attached := false
var _bone := &""
var _slot := 0
var _offset_position := Vector3()
var _offset_rotation := Quaternion()
var _provider: BoneAttachment3D = null
var _saved_parent: Node = null
var _warned_missing_socket := false

var _state_dirty := false
var _out_flags := 0

var _listening := false
var _published_key := -1
var _retry_accum := 0.0
var _retry_count := 0
var _resolving := false


func _ready() -> void:
	_bone = bone_name
	_slot = socket_slot
	_offset_position = offset_position
	_offset_rotation = offset_rotation
	if role == Role.SOCKET_PROVIDER:
		if not (_parent_node() is BoneAttachment3D):
			push_warning("CYGBoneAttachmentSync provider '%s' is not a child of a BoneAttachment3D; nothing to publish." % name)
			return
		# A socket exists as soon as the avatar scene does, so publishing happens here:
		# the view's spawn hooks walk direct children only, and a provider sits under
		# the skeleton. The signal connections keep later spawns and owner changes fresh.
		if view != null:
			_register_provider()
		return
	# Registration is unconditional and identical on every peer.
	register_as_observable()


func _exit_tree() -> void:
	if role == Role.SOCKET_PROVIDER:
		_unregister_provider()
	super._exit_tree()


## ----------------------------------------------------------------- provider

func cyg_on_spawned() -> void:
	# Direct view children only: a provider lives under the Skeleton3D, so for it
	# this runs only when invoked manually; `_on_avatar_spawned` covers the real
	# avatar path.
	if role == Role.SOCKET_PROVIDER:
		_register_provider()


func cyg_on_owner_changed(_new_owner: CYGPlayer) -> void:
	if role == Role.SOCKET_PROVIDER:
		_reregister_provider()


func _on_avatar_spawned() -> void:
	_register_provider()


func _on_avatar_owner_changed(_new_owner: CYGPlayer) -> void:
	_reregister_provider()


func _register_provider() -> void:
	var attachment := _parent_node() as BoneAttachment3D
	if attachment == null:
		return
	var target := view
	if target == null:
		return
	if not _listening:
		# The view's hooks walk direct children only; signals reach a deep node.
		target.spawned.connect(_on_avatar_spawned)
		target.owner_changed.connect(_on_avatar_owner_changed)
		_listening = true
	var by_slot: Dictionary = _providers.get(target.owner_actor, {})
	_published_key = _provider_key(attachment)
	by_slot[_published_key] = weakref(attachment)
	_providers[target.owner_actor] = by_slot


func _reregister_provider() -> void:
	_unregister_provider()
	_register_provider()


func _unregister_provider() -> void:
	var target := view
	if target == null:
		return
	var by_slot: Dictionary = _providers.get(target.owner_actor, {})
	# `_exit_tree` runs after the parent is gone, so the key is the one this provider
	# actually published rather than one recomputed from a node it can no longer see.
	by_slot.erase(_published_key)
	if by_slot.is_empty():
		_providers.erase(target.owner_actor)
	_published_key = -1


## The registry key this provider owns: the slot it was authored with, plus the bone
## it is attached to when no `bone_name` override was set.
func _provider_key(attachment: BoneAttachment3D) -> int:
	var bone: StringName = _bone
	if bone == &"":
		bone = attachment.bone_name
	return _slot_key(_slot, bone)


## ------------------------------------------------------------- attachment

## True while this node is parented under a socket.
var is_attached: bool:
	get:
		return _attached


## Equip on the owner's avatar bone; authority-only, proxies receive the state.
func attach_to_bone(bone: StringName, slot: int = -1) -> void:
	if not has_authority:
		push_warning("CYGBoneAttachmentSync.attach_to_bone('%s') ignored: no authority." % bone)
		return
	var next_slot := socket_slot if slot < 0 else slot
	if _bone == bone and _slot == next_slot and _attached:
		return
	_bone = bone
	_slot = next_slot
	_attached = true
	_out_flags |= FLAG_STATE | FLAG_BONE
	_state_dirty = true
	_resolving = true
	_retry_accum = 0.0
	_retry_count = 0
	_warned_missing_socket = false
	_try_resolve()


## Unequip: the node returns to its pre-attach parent with its local transform.
func detach() -> void:
	if not has_authority:
		push_warning("CYGBoneAttachmentSync.detach() ignored: no authority.")
		return
	if not _attached:
		return
	_attached = false
	_out_flags |= FLAG_STATE
	_state_dirty = true
	_resolving = false
	_reparent_back()


## Moves the local socket offset; syncs with FLAG_OFFSET.
func set_local_offset(position: Vector3, rotation: Quaternion) -> void:
	if not has_authority:
		push_warning("CYGBoneAttachmentSync.set_local_offset() ignored: no authority.")
		return
	_offset_position = position
	_offset_rotation = rotation
	_out_flags |= FLAG_OFFSET
	_state_dirty = true
	if _provider != null and is_instance_valid(_provider):
		var weapon := _weapon_node()
		if weapon != null:
			weapon.position = position
			weapon.quaternion = rotation


func has_delta() -> bool:
	if role == Role.SOCKET_PROVIDER:
		return false
	return _state_dirty or super.has_delta()


## Provider role writes nothing and must be byte-symmetric with reading nothing;
## the role is scene-fixed, so both peers take the same branch.
func serialize(writer: CYGBitPacker) -> void:
	if role == Role.SOCKET_PROVIDER:
		return
	super.serialize(writer)
	# The flags byte is written even when zero so deserialize's fixed layout stays
	# symmetric with property-only deltas.
	writer.write_byte(_out_flags)
	if not _state_dirty:
		return

	if (_out_flags & FLAG_STATE) != 0:
		writer.write_byte(_slot)
		writer.write_bool(_attached)
	if (_out_flags & FLAG_BONE) != 0:
		writer.write_string(String(_bone))
	if (_out_flags & FLAG_OFFSET) != 0:
		writer.write_vector3(_offset_position)
		writer.write_quaternion(_offset_rotation)
	_out_flags = 0
	_state_dirty = false


func deserialize(reader: CYGBitReader) -> void:
	super.deserialize(reader)
	if role == Role.SOCKET_PROVIDER:
		return

	var flags := reader.read_byte()
	# Consume everything serialize committed, on every peer, so the shared
	# reader stays aligned for the components behind this one.
	var slot := _slot
	var attached := _attached
	var bone := _bone
	var position := _offset_position
	var rotation := _offset_rotation
	if (flags & FLAG_STATE) != 0:
		slot = reader.read_byte()
		attached = reader.read_bool()
	if (flags & FLAG_BONE) != 0:
		bone = StringName(reader.read_string())
	if (flags & FLAG_OFFSET) != 0:
		position = reader.read_vector3()
		rotation = reader.read_quaternion()

	# The authority decoded the block only to keep the reader aligned; its own
	# equip calls already hold the state.
	if has_authority:
		return

	_bone = bone
	_slot = slot
	_offset_position = position
	_offset_rotation = rotation
	if _attached == attached:
		return
	_attached = attached
	if attached:
		_resolving = true
		_retry_accum = 0.0
		_retry_count = 0
		_warned_missing_socket = false
		_try_resolve()
	else:
		_resolving = false
		_reparent_back()


## Retry socket resolution until the avatar exists; a late joiner spawns the
## weapon before its carrier's avatar has reached this peer.
func _process(delta: float) -> void:
	if not _resolving or not _attached:
		return
	_retry_accum += delta
	if _retry_accum < 0.25:
		return
	_retry_accum = 0.0
	_retry_count += 1
	_try_resolve()


func _try_resolve() -> void:
	var target := view
	if target == null:
		return
	var by_slot: Dictionary = _providers.get(target.owner_actor, {})
	var candidate: BoneAttachment3D = null
	var handle: Variant = by_slot.get(_slot_key(_slot, _bone))
	if handle is WeakRef:
		candidate = handle.get_ref() as BoneAttachment3D
	if candidate == null:
		if not _warned_missing_socket and _retry_count >= 16:
			_warned_missing_socket = true
			push_warning("CYGBoneAttachmentSync: no socket '%s' slot %d for actor %d; the weapon stays where it spawned."
				% [_bone, _slot, target.owner_actor])
		return
	_resolving = false
	_provider = candidate
	var weapon := _weapon_node()
	if weapon == null:
		return
	if _saved_parent == null:
		_saved_parent = weapon.get_parent()
	# keep_global_transform = false: the offset authored here IS the local pose.
	weapon.reparent(candidate, false)
	weapon.position = _offset_position
	weapon.quaternion = _offset_rotation


func _reparent_back() -> void:
	var weapon := _weapon_node()
	if weapon == null or _saved_parent == null or not is_instance_valid(_saved_parent):
		return
	weapon.reparent(_saved_parent, false)
	_provider = null


func _weapon_node() -> Node3D:
	var target := view
	var weapon := target.network_node as Node3D if target != null else null
	if weapon == null and _parent_node() is Node3D:
		# Component placed as a grandchild: sync the nearest Node3D ancestor.
		weapon = _parent_node() as Node3D
	return weapon


func _parent_node() -> Node:
	var parent := get_parent()
	if parent == null:
		push_error("CYGBoneAttachmentSync on %s: no parent node." % name)
	return parent


static func _slot_key(slot: int, bone: StringName) -> int:
	# Slot and bone both identify a socket; hashing them avoids nested
	# dictionaries that would allocate on every registration.
	return (slot * 104729) ^ String(bone).hash()
