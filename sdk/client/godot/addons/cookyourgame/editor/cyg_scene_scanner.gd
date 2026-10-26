class_name CYGSceneScanner
extends RefCounted
## Static structural audit of a scene that uses the SDK's components.
##
## Every rule here is a mistake the runtime would otherwise only reveal in a
## playtest: a view whose node cannot carry a transform, two components that both
## write one pose, a behaviour that belongs to no view, a duplicate `view_id`. The
## editor dock runs this on demand; games can call it from a debug menu.
##
## Input is any `Node` that owns the subtree to audit, normally an edited scene
## root. Output is an `Array` of `Dictionary`, one per finding, in a stable order:
##
##     { "path": String  node path relative to the scanned root,
##       "code": String  stable identifier, safe to match on
##       "severity": int SEVERITY_ERROR or SEVERITY_WARNING,
##       "message": String what to change }
##
## Errors are states the SDK cannot recover from at runtime; warnings are legal
## configurations that are almost always a mistake.

const SEVERITY_ERROR := 0
const SEVERITY_WARNING := 1

const _ISSUE_PATH := "path"
const _ISSUE_CODE := "code"
const _ISSUE_SEVERITY := "severity"
const _ISSUE_MESSAGE := "message"

## Components that each claim sole ownership of their node's pose. Two of these on
## one node means the last writer wins and the other's interpolation is invisible.
const _POSE_WRITERS := [
	"CYGTransformSync3D",
	"CYGTransformSync2D",
	"CYGPhysicsSync3D",
	"CYGPhysicsSync2D",
	"CYGCharacterSync3D",
	"CYGCharacterSync2D",
]

## Which body class a sync component drives, by class name. A mismatch is silent at
## runtime (the component simply finds no body), so it has to be caught statically.
const _REQUIRED_BODY := {
	"CYGPhysicsSync3D": "RigidBody3D",
	"CYGPhysicsSync2D": "RigidBody2D",
	"CYGCharacterSync3D": "CharacterBody3D",
	"CYGCharacterSync2D": "CharacterBody2D",
	"CYGTransformSync3D": "Node3D",
	"CYGTransformSync2D": "Node2D",
}


## Audits `root` and every node below it. `root` itself is included.
static func scan(root: Node) -> Array:
	var issues: Array = []
	if root == null:
		issues.append(_issue("", "NO_ROOT", SEVERITY_ERROR, "Nothing to scan: no scene is open."))
		return issues

	var views: Array = []
	var seen_ids := {}
	_collect(root, root, issues, views, seen_ids)
	_check_pose_writers(views, root, issues)
	issues.sort_custom(_by_severity_then_path)
	return issues


## Count of findings by severity, for a one-line summary: `{error: 2, warning: 1}`.
static func count_by_severity(issues: Array) -> Dictionary:
	var counts := {"error": 0, "warning": 0}
	for issue in issues:
		if int(issue[_ISSUE_SEVERITY]) == SEVERITY_ERROR:
			counts["error"] = int(counts["error"]) + 1
		else:
			counts["warning"] = int(counts["warning"]) + 1
	return counts


## The single line a status label shows.
static func summarize(issues: Array) -> String:
	var counts := count_by_severity(issues)
	if int(counts["error"]) == 0 and int(counts["warning"]) == 0:
		return "No issues found."
	return "%d error(s), %d warning(s)" % [counts["error"], counts["warning"]]


static func _collect(node: Node, root: Node, issues: Array, views: Array, seen_ids: Dictionary) -> void:
	if node is CYGView:
		_check_view(node as CYGView, root, issues, views, seen_ids)
	elif node is CYGBehaviour:
		# The same walk the runtime uses: a component finds its view on the parent, as a
		# sibling, or by climbing. Nothing found means it can never be told apart.
		if (node as CYGBehaviour).view == null:
			issues.append(_issue(_path_of(node, root), "NO_VIEW", SEVERITY_ERROR,
				"%s has no CYGView above it or beside it, so it will never sync." % _class_of(node)))
	for child in node.get_children():
		_collect(child, root, issues, views, seen_ids)


static func _check_view(view: CYGView, root: Node, issues: Array, views: Array, seen_ids: Dictionary) -> void:
	var where := _path_of(view, root)
	var carrier := view.network_node
	if carrier == null:
		issues.append(_issue(where, "ORPHAN_VIEW", SEVERITY_ERROR,
			"CYGView must be a child of the node it networks."))
		return
	if not (carrier is Node2D or carrier is Node3D):
		# Position is the payload every sync component produces; a Control or a plain
		# Node has no world transform to replicate.
		issues.append(_issue(where, "UNTRANSFORMABLE_NODE", SEVERITY_ERROR,
			"%s is not a Node2D/Node3D, so nothing can be synced about it." % carrier.name))
	if view.view_id > 0:
		if seen_ids.has(view.view_id):
			issues.append(_issue(where, "DUPLICATE_VIEW_ID", SEVERITY_ERROR,
				"view_id %d is already used by %s; two views with one id overwrite each other's state."
					% [view.view_id, str(seen_ids[view.view_id])]))
		else:
			seen_ids[view.view_id] = _path_of(carrier, root)
	if view.observed_components.size() > view.MAX_OBSERVED_COMPONENTS:
		issues.append(_issue(where, "TOO_MANY_OBSERVABLES", SEVERITY_ERROR,
			"%d observable components exceed the %d the dirty mask can address; drop one or split the object."
				% [view.observed_components.size(), view.MAX_OBSERVED_COMPONENTS]))
	if view.observed_components.is_empty():
		issues.append(_issue(where, "SILENT_VIEW", SEVERITY_WARNING,
			"%s has no observable component: the view replicates identity only (fine for RPC-only objects)."
				% carrier.name))
	for component in view.observed_components:
		if not (component is Node) or not is_instance_valid(component):
			issues.append(_issue(where, "BROKEN_OBSERVABLE", SEVERITY_ERROR,
				"%s lists an observable that is not a live node." % carrier.name))
			continue
		if component.get_parent() != carrier:
			# Registration order is the wire identity, and the view only walks its own
			# children at spawn, so a nested observable syncs but never gets its hooks.
			issues.append(_issue(_path_of(component as Node, root), "OBSERVABLE_NOT_CHILD", SEVERITY_WARNING,
				"%s is registered on %s but is not a direct child; spawn hooks will not reach it."
					% [component.name, carrier.name]))
		_check_body_match(component as Node, carrier, root, issues)
		if component is CYGBoneAttachmentSync \
				and (component as CYGBoneAttachmentSync).role == CYGBoneAttachmentSync.Role.SOCKET_PROVIDER:
			# A provider publishes a node reference locally and writes zero bytes; giving
			# it a dirty-mask slot costs a bit per tick and ships nothing.
			issues.append(_issue(_path_of(component as Node, root), "PROVIDER_OBSERVED", SEVERITY_ERROR,
					"%s is a socket provider and must not be observed; it has no payload." % component.name))
	views.append(view)


## The engine class a sync component needs on its node, checked against what is
## actually there.
static func _check_body_match(component: Node, carrier: Node, root: Node, issues: Array) -> void:
	var required: String = _REQUIRED_BODY.get(_class_of(component), "")
	if required.is_empty():
		return
	if not ClassDB.is_parent_class(carrier.get_class(), required):
		issues.append(_issue(_path_of(component, root), "WRONG_BODY", SEVERITY_ERROR,
			"%s networks %s, but %s is a %s." % [_class_of(component), required, carrier.name, carrier.get_class()]))


## Two pose writers on one node: the loser's work is invisible and its bandwidth is
## wasted, so this is always a mistake.
static func _check_pose_writers(views: Array, root: Node, issues: Array) -> void:
	for view in views:
		var carrier := (view as CYGView).network_node
		if carrier == null:
			continue
		var writers: Array = []
		for component in (view as CYGView).observed_components:
			if component is Node and _POSE_WRITERS.has(_class_of(component as Node)):
				writers.append(_class_of(component as Node))
		if writers.size() > 1:
			issues.append(_issue(_path_of(carrier, root), "DOUBLE_POSE_WRITER", SEVERITY_ERROR,
				"%s is written by %s; keep one pose owner per node."
					% [_path_of(carrier, root), ", ".join(PackedStringArray(writers))]))


## The SDK class a node is, for matching against the tables above. `Object.get_class()`
## answers with the nearest *engine* base, so a `CYGPhysicsSync3D` reports "Node" and
## every rule keyed by class name would silently miss.
static func _class_of(node: Node) -> String:
	var script := node.get_script() as Script
	if script != null:
		var global_name := String(script.get_global_name())
		if not global_name.is_empty():
			return global_name
	return node.get_class()


static func _path_of(node: Node, root: Node) -> String:
	if node == null:
		return ""
	if node == root:
		return String(node.name)
	var tail := _relative(node, root)
	return tail if not tail.is_empty() else String(node.name)


static func _relative(node: Node, root: Node) -> String:
	var segments: Array[String] = []
	var cursor: Node = node
	while cursor != null and cursor != root:
		segments.push_front(String(cursor.name))
		cursor = cursor.get_parent()
	return "/".join(PackedStringArray(segments))


static func _issue(path: String, code: String, severity: int, message: String) -> Dictionary:
	return {
		_ISSUE_PATH: path,
		_ISSUE_CODE: code,
		_ISSUE_SEVERITY: severity,
		_ISSUE_MESSAGE: message,
	}


static func _by_severity_then_path(a: Dictionary, b: Dictionary) -> bool:
	if int(a[_ISSUE_SEVERITY]) != int(b[_ISSUE_SEVERITY]):
		return int(a[_ISSUE_SEVERITY]) < int(b[_ISSUE_SEVERITY])
	return str(a[_ISSUE_PATH]) < str(b[_ISSUE_PATH])
