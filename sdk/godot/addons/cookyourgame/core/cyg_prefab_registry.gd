class_name CYGPrefabRegistry
extends Resource
## Maps numeric prefab ids (the wire identity) to `PackedScene` resources.
##
## Ids must agree across every client — the room relays spawn packets verbatim,
## so a `prefabId` means nothing without a local registry entry. Declare the
## list once (editor resource) and share it, exactly like Unity's
## `CYGPrefabRegistry`.

@export var entries: Array[CYGPrefabEntry] = []


func get_prefab_id(prefab_name: String) -> int:
	for entry in entries:
		if entry.name == prefab_name:
			return entry.id
	push_error("CYGPrefabRegistry: no prefab named '%s'." % prefab_name)
	return CYGNetwork.INVALID_PREFAB_ID


func get_prefab(prefab_id: int) -> PackedScene:
	for entry in entries:
		if entry.id == prefab_id:
			if entry.scene == null:
				push_error("CYGPrefabRegistry: prefab id %d has no scene assigned." % prefab_id)
				return null
			return entry.scene
	push_error("CYGPrefabRegistry: unknown prefab id %d." % prefab_id)
	return null
