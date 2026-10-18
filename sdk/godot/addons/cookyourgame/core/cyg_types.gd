class_name CYGTypes
## Shared data shapes for the CookYourGame SDK.

## Coordinates of a reserved seat, as returned by `POST /matchmake/<m>/<room>`.
class Seat:
	extends RefCounted
	var name: String = ""
	var session_id: String = ""
	var room_id: String = ""
	var process_id: String = ""
	var reconnection_token: String = ""
	var game_port: int = 0
	var error: String = ""

	var ok: bool:
		get:
			return not session_id.is_empty()

	func _to_string() -> String:
		if not ok:
			return "Seat(error=%s)" % error
		return "Seat(room=%s session=%s process=%s port=%d)" % [
			room_id, session_id, process_id, game_port
		]


## Summary of a room as surfaced by room logic or the console.
class RoomInfo:
	extends RefCounted
	var room_id: String = ""
	var name: String = ""
	var clients: int = 0
	var max_clients: int = 0
	var tick: int = 0


## A registered network prefab: the id travels on the wire, the resource stays
## client-side, exactly like Unity's `CYGPrefabRegistry`.
class PrefabEntry:
	extends RefCounted
	var id: int = 0
	var name: String = ""
	var scene: PackedScene
