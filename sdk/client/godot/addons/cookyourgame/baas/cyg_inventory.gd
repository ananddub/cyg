class_name CYGInventory
extends RefCounted
## Server-authoritative inventory for the CookYourGame Godot SDK: `/v2/inventory*`.
##
## The rows are owned by the engine; this class only mirrors them. `items` is that
## mirror, keyed with the wire's own field names, and every method answers through
## the callback it was handed so nothing here blocks a game thread.
##
## Endpoints, field by field (see `crates/engine-api/src/controllers/inventory.rs`,
## `crates/engine-api/src/dto/inventory.rs`, `src/services/inventory/mod.rs` and
## `repositories/models.rs` for the row shape):
##
##  * `GET /v2/inventory` — the caller's whole bag. Reply is `ListInventoryResponse`
##    `{"items": [...]}`, where each entry is a `PlayerInventoryItem`:
##
##        {"instance_id": str(uuid), "user_id": str(uuid), "item_id": str,
##         "quantity": int, "slot_index": int, "metadata": object,
##         "created_at": str(RFC3339), "updated_at": str(RFC3339)}
##
##    `instance_id` is the per-copy identity every other call takes; `item_id` is
##    the definition's slug. `metadata` is a `serde_json::Value` bound from a jsonb
##    column, so it arrives as a JSON OBJECT, not as text — `Unity`'s
##    `CYGInventoryItem.metadata` is declared `string`, which is the one place the
##    C# mirror cannot read what this server writes; this class follows the server.
##    `slot_index` is `-1` for anything not equipped.
##
##  * `POST /v2/inventory/equip` — `{"instance_id": uuid, "slot_index": int}`,
##    moving one owned copy into a slot (0 = main hand, 1 = helmet, and so on; the
##    engine only checks ownership, the slot number is the game's own scheme).
##  * `POST /v2/inventory/consume` — `{"instance_id": uuid, "quantity": int}`,
##    `quantity` must be at least 1 and at most what the row holds, and BOTH are
##    required fields (`i32`, not `Option`), so a consume with no count is a 400.
##  * `POST /v2/inventory/grant` — `{"item_id": str, "quantity"?: int,
##    "slot_index"?: int, "metadata"?: object, "user_id"?: uuid}`. `quantity` is
##    clamped to `1..the definition's max_stack`, `slot_index` defaults to `-1`,
##    `user_id` defaults to the caller. A grant of an undefined `item_id` is a 404
##    (`item definition '...' not found`), so `define()` must run first.
##  * `POST /v2/inventory/define` — `{"item_id": str, "name": str, "category": str,
##    "max_stack"?: int, "rarity"?: str, "properties"?: object}`; `max_stack`
##    defaults to 1 (and never below it), `rarity` to `"common"`, `properties` to
##    `{}`. It upserts the definition. The route carries no `Authed` extractor, so
##    it is a control-plane call: any token, or none, reaches it.
##
## The last three answer `InventoryOperationResponse`:
##
##     {"success": bool, "message": str, "item": object|null}
##
## `item` is always PRESENT in the JSON and `null` when there is no row to report —
## a `define`, and a `consume` that spent the last of a stack, because the engine
## DELETEs the row at zero (`update_inventory_quantity`). An absent `item` after a
## consume therefore drops that `instance_id` from `items` instead of leaving a
## stale stack behind. A refusal never reaches this shape: `/v2/` answers
## `{"error": {"code", "message", "details"}}` with a 4xx, which surfaces in
## `error` with `ok` false.
##
## All callbacks receive one Dictionary, `ok` first and never absent:
##
##     {"ok": bool, "status": int, "error": String, "result": Variant,
##      "success": bool, "message": String, "item": Dictionary}
##
## `ok` is the transport verdict (HTTP 2xx and no transport error), `success` the
## engine's own verdict from the DTO, and `message` its text. `result` is `items`
## for `fetch()` and the reported `item` (empty when the row is gone) for an
## operation. `status` is 0 when the request never reached the server.
##
## Usage:
##
##     CYGInventory.on_inventory_changed = func(list: Array) -> void:
##         _refresh_hud(list)
##
##     CYGInventory.fetch(func(r: Dictionary) -> void:
##         if not r["ok"]:
##             _show_retry(r["error"])
##             return
##         for item in CYGInventory.items:
##             print(item["item_id"], " x", item["quantity"]))
##
##     var sword := CYGInventory.get_item_in_slot(0)
##     if not sword.is_empty():
##         CYGInventory.consume(str(sword["instance_id"]), 1, func(r: Dictionary) -> void:
##             _set_ammo_count(int(r["item"].get("quantity", 0))))
##
## Every call goes through `CYGHttp`, the BaaS layer's one transport: it resolves the
## endpoint, attaches the session `CYGAuth` published, and retries a request that never
## got a reply, so a bag fetch and an equip cannot disagree about any of that.
##
## Independent port of this repo's Unity BaaS layer
## (`sdk/unity/Runtime/BaaS/CYGInventory.cs`) onto the documented `/v2/` endpoints
## of the Rust engine-api; no upstream source text is copied.

const PATH_ROOT := "/v2/inventory"
const PATH_EQUIP := "/v2/inventory/equip"
const PATH_CONSUME := "/v2/inventory/consume"
const PATH_GRANT := "/v2/inventory/grant"
const PATH_DEFINE := "/v2/inventory/define"

## `slot_index` a row carries when it is in the bag rather than equipped.
const SLOT_NOT_EQUIPPED := -1

## Counterpart of Unity's `OnInventoryChanged` event. A static facade cannot own a
## signal, so this is a `Callable` hook, the same idiom `CYGClient.on_ping_sent`
## uses. Called with the refreshed `items` after a fetch or a mutating operation.
##
## Bind a node method (`CYGInventory.on_inventory_changed = hud.refresh`) or clear
## it before the process exits. A lambda that captures locals and is still sitting
## in this static var at shutdown double-frees during Godot 4.7's static cleanup —
## seen here as `double free or corruption` after an otherwise clean run.
static var on_inventory_changed: Callable = Callable()

## The caller's own rows, with the DTO's field names: `instance_id`, `user_id`,
## `item_id`, `quantity`, `slot_index`, `metadata`, `created_at`, `updated_at`.
static var items: Array[Dictionary] = []


## Pulls the authoritative bag and replaces `items` with it. On a failure `items`
## keeps its previous contents, as `CYGInventory.GetInventoryAsync` returns the
## stale cache rather than an empty one.
static func fetch(callback: Callable) -> void:
	_request(HTTPClient.METHOD_GET, PATH_ROOT, "", true,
		func(status: int, text: String, transport_error: String) -> void:
			var reply := _reply_dict(transport_ok(status, transport_error), status,
					"" if transport_ok(status, transport_error) else _first_non_empty(transport_error, error_message_of(text)))
			if reply["ok"]:
				var parsed := JSON.parse_string(text)
				if typeof(parsed) == TYPE_DICTIONARY:
					_replace_cache((parsed as Dictionary).get("items", []))
					reply["result"] = items
				else:
					reply["ok"] = false
					reply["error"] = "inventory list reply was not a JSON object: " + text
			_deliver(callback, reply))


## Moves one owned copy into `slot_index`. See `PATH_EQUIP` above; the engine
## refuses an instance the caller does not own (403) and an unknown one (404).
static func equip(instance_id: String, slot_index: int, callback: Callable) -> void:
	_post_op(PATH_EQUIP, {"instance_id": instance_id, "slot_index": slot_index},
			instance_id, callback)


## Spends `quantity` of one instance. The server requires a positive count and
## fails the call when the stack is smaller than it.
static func consume(instance_id: String, quantity: int, callback: Callable) -> void:
	if quantity < 1:
		_deliver(callback, _reply_dict(false, 0, "quantity to consume must be at least 1."))
		return
	_post_op(PATH_CONSUME, {"instance_id": instance_id, "quantity": quantity},
			instance_id, callback)


## Grants a copy of a defined item, optionally straight into a slot and with
## per-copy `metadata`. `quantity` is clamped by the server to the definition's
## `max_stack`; `user_id` targets another account when the caller may grant one.
static func grant(
		item_id: String,
		quantity: int,
		callback: Callable,
		slot_index: int = SLOT_NOT_EQUIPPED,
		metadata: Dictionary = {},
		user_id: String = ""
) -> void:
	var payload := {"item_id": item_id, "quantity": quantity, "slot_index": slot_index}
	if not metadata.is_empty():
		payload["metadata"] = metadata
	if not user_id.is_empty():
		payload["user_id"] = user_id
	_post_op(PATH_GRANT, payload, "", callback)


## Registers (upserts) an item definition, which `grant()` requires to exist.
## Answers `result` with no `item`, since a definition is not a row in a bag.
static func define(
		item_id: String,
		item_name: String,
		category: String,
		callback: Callable,
		max_stack: int = 1,
		rarity: String = "common",
		properties: Dictionary = {}
) -> void:
	var payload := {"item_id": item_id, "name": item_name, "category": category,
			"max_stack": max_stack, "rarity": rarity}
	if not properties.is_empty():
		payload["properties"] = properties
	# `define` is the one inventory route the engine reads without a session: its
	# handler takes no `Authed` extractor, so it is keyed by the server credential.
	_post_op(PATH_DEFINE, payload, "", callback, false)


## The copy equipped in `slot_index`, or `{}` when the slot is empty. Mirrors
## `CYGInventory.GetItemInSlot`, whose `null` becomes an empty Dictionary here.
static func get_item_in_slot(slot_index: int) -> Dictionary:
	for item in items:
		if int(item.get("slot_index", SLOT_NOT_EQUIPPED)) == slot_index:
			return item
	return {}


## The copy with this per-instance uuid, or `{}` when the bag has no such row.
static func get_item_by_instance_id(instance_id: String) -> Dictionary:
	for item in items:
		if str(item.get("instance_id", "")) == instance_id:
			return item
	return {}


## Forgets the mirrored bag (on a logout, say) without touching the server.
static func clear_cache() -> void:
	items.clear()
	_notify_changed()


## One POST that answers `InventoryOperationResponse`, then folds the reported row
## into `items`. `removes` names the instance a spent-away row should drop.
static func _post_op(path: String, payload: Dictionary, removes: String, callback: Callable,
		require_auth: bool = true) -> void:
	_request(HTTPClient.METHOD_POST, path, JSON.stringify(payload), require_auth,
		func(status: int, text: String, transport_error: String) -> void:
			var ok: bool = transport_ok(status, transport_error)
			var reply := _reply_dict(ok, status, "" if ok else _first_non_empty(transport_error, error_message_of(text)))
			reply["success"] = false
			reply["message"] = ""
			reply["item"] = {}
			if ok:
				var parsed := JSON.parse_string(text)
				if typeof(parsed) == TYPE_DICTIONARY:
					var response := parsed as Dictionary
					var succeeded: bool = bool(response.get("success", false))
					reply["success"] = succeeded
					reply["message"] = str(response.get("message", ""))
					var reported: Variant = response.get("item", null)
					if typeof(reported) == TYPE_DICTIONARY:
						var item := _normalize_item(reported as Dictionary)
						reply["item"] = item
						reply["result"] = item
						if succeeded:
							_upsert_item(item)
					else:
						# `item` is null: a define, or a consume that spent the
						# last of the stack and had its row deleted.
						reply["result"] = {}
						if succeeded and not removes.is_empty():
							_remove_item(removes)
				else:
					reply["ok"] = false
					reply["error"] = "inventory operation reply was not a JSON object: " + text
			_deliver(callback, reply))


static func transport_ok(status: int, transport_error: String) -> bool:
	var in_range: bool = status >= 200 and status <= 299
	return in_range and transport_error.is_empty()


## Wire row to cache row. The DTO always emits every field, so this only fills the
## two whose absence would otherwise read as a broken stack.
static func _normalize_item(raw: Dictionary) -> Dictionary:
	var item := raw.duplicate()
	item["quantity"] = int(raw.get("quantity", 0))
	item["slot_index"] = int(raw.get("slot_index", SLOT_NOT_EQUIPPED))
	if typeof(raw.get("metadata", null)) != TYPE_DICTIONARY:
		item["metadata"] = {}
	return item


static func _replace_cache(raw: Variant) -> void:
	items.clear()
	if typeof(raw) == TYPE_ARRAY:
		for entry in raw as Array:
			if typeof(entry) == TYPE_DICTIONARY:
				items.append(_normalize_item(entry as Dictionary))
	_notify_changed()


static func _upsert_item(item: Dictionary) -> void:
	var instance_id := str(item.get("instance_id", ""))
	for index in range(items.size()):
		if str(items[index].get("instance_id", "")) == instance_id:
			items[index] = item
			_notify_changed()
			return
	items.append(item)
	_notify_changed()


static func _remove_item(instance_id: String) -> void:
	for index in range(items.size() - 1, -1, -1):
		if str(items[index].get("instance_id", "")) == instance_id:
			items.remove_at(index)
			_notify_changed()
			return


static func _notify_changed() -> void:
	if on_inventory_changed.is_valid():
		on_inventory_changed.call(items.duplicate())


static func _reply_dict(ok: bool, status: int, error: String) -> Dictionary:
	return {"ok": ok, "status": status, "error": error, "result": null,
			"success": false, "message": "", "item": {}}


static func _deliver(callback: Callable, reply: Dictionary) -> void:
	if callback.is_valid():
		callback.call(reply)


static func _first_non_empty(first: String, second: String) -> String:
	return first if not first.is_empty() else second


## A `/v2/` refusal is `{"error": {"code", "message", "details"}}`
## (`error/v2.rs`); anything else is surfaced as the raw body.
static func error_message_of(text: String) -> String:
	if text.is_empty():
		return ""
	var parsed := JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		return text
	var details: Variant = (parsed as Dictionary).get("error", null)
	if typeof(details) != TYPE_DICTIONARY:
		return text
	var message := str((details as Dictionary).get("message", ""))
	return message if not message.is_empty() else text


## Runs one call through `CYGHttp` and keeps this class's own reply shape: `status` is
## the HTTP code, or 0 when no reply arrived, in which case `transport_error` says why.
## `require_auth` picks the credential: a bearer session for the player's own rows, the
## server key for the route the engine reads without one.
static func _request(method: int, path: String, body: String, require_auth: bool,
		on_done: Callable) -> void:
	CYGHttp.send_method(method, path, body, require_auth,
		func(response: CYGHttp.Response) -> void:
			on_done.call(response.status_code, response.text,
				"" if response.reached_server else response.error))
